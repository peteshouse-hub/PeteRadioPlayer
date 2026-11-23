#include "StationDirectory.h"

#include <ArduinoJson.h>
#include <HTTPClient.h>
#include <NetworkClientSecure.h>
#include <StreamString.h>
#include <atomic>
#include <time.h>

#include "DirectoryTrust.h"

namespace {
SearchStation results[MaxSearchResults];
String searchTerm;
int resultCount;
std::atomic<SearchStatus> state{SearchStatus::Idle};

class DirectoryResponse : public StreamString {
public:
  static constexpr size_t MaxBytes = 96 * 1024;

  size_t write(const uint8_t* bytes, size_t size) override {
    if (size > MaxBytes - length()) return 0;
    return StreamString::write(bytes, size);
  }
};

String encodeQuery(const String& term) {
  String encoded;
  for (unsigned char ch : term) {
    if (isalnum(ch) || ch == '-') {
      encoded += static_cast<char>(ch);
    } else {
      char escape[4];
      snprintf(escape, sizeof(escape), "%%%02X", static_cast<unsigned>(ch));
      encoded += escape;
    }
  }
  return encoded;
}

bool httpUrl(const String& url) {
  return url.length() <= 1024 && (url.startsWith("https://") || url.startsWith("http://"));
}

SearchStatus fetchStations() {
  const uint32_t started = millis();
  while (time(nullptr) < 1700000000 && millis() - started < 6000) delay(100);
  if (time(nullptr) < 1700000000) return SearchStatus::ClockUnavailable;

  NetworkClientSecure client;
  client.setCACert(DirectoryTrust);
  client.setHandshakeTimeout(5);
  HTTPClient http;
  http.setConnectTimeout(4000);
  http.setTimeout(5000);
  http.setUserAgent("RadioKing-ESP32/0.2");
  const String url = "https://de1.api.radio-browser.info/json/stations/search?name="
                     + encodeQuery(searchTerm)
                     + "&limit=40&hidebroken=true&order=votes&reverse=true";
  if (!http.begin(client, url)) return SearchStatus::Unavailable;
  const int code = http.GET();
  if (code != HTTP_CODE_OK) {
    char tlsError[112]{};
    client.lastError(tlsError, sizeof(tlsError));
    Serial.printf("Radio Browser HTTP status %d, TLS %s\n", code, tlsError);
    return SearchStatus::Unavailable;
  }
  DirectoryResponse response;
  const int contentLength = http.getSize();
  if (contentLength > static_cast<int>(DirectoryResponse::MaxBytes) ||
      (contentLength > 0 && !response.reserve(contentLength))) return SearchStatus::Unavailable;
  const int received = http.writeToStream(&response);
  if (received <= 0) {
    Serial.printf("Radio Browser response error: %d\n", received);
    return SearchStatus::Unavailable;
  }

  JsonDocument filter;
  filter[0]["name"] = true;
  filter[0]["url"] = true;
  filter[0]["url_resolved"] = true;
  filter[0]["codec"] = true;
  JsonDocument stations;
  const auto error = deserializeJson(stations, response.c_str(), DeserializationOption::Filter(filter));
  if (error || !stations.is<JsonArray>()) {
    Serial.printf("Radio Browser JSON error: %s\n", error.c_str());
    return SearchStatus::Unavailable;
  }

  for (JsonObject station : stations.as<JsonArray>()) {
    if (resultCount == MaxSearchResults) break;
    String codec = station["codec"] | "";
    codec.toUpperCase();
    if (!codec.isEmpty() && codec != "MP3" && codec != "AAC" && codec != "AAC+" &&
        codec != "FLAC" && codec != "OPUS" && codec != "OGG" && codec != "VORBIS") continue;

    String stream = station["url_resolved"] | "";
    if (!httpUrl(stream)) stream = station["url"] | "";
    if (!httpUrl(stream)) continue;
    String name = station["name"] | "";
    name.trim();
    if (name.isEmpty()) continue;
    results[resultCount++] = {name.substring(0, 120), stream};
  }
  return SearchStatus::Complete;
}

void searchTask(void*) {
  const SearchStatus outcome = fetchStations();
  state.store(outcome, std::memory_order_release);
  vTaskDelete(nullptr);
}
}

bool startStationSearch(const String& query) {
  if (stationSearchStatus() == SearchStatus::Running) return false;
  for (int i = 0; i < resultCount; ++i) results[i] = {};
  resultCount = 0;
  searchTerm = query;
  state.store(SearchStatus::Running, std::memory_order_relaxed);
  if (xTaskCreate(searchTask, "station search", 12288, nullptr, 1, nullptr) != pdPASS) {
    state.store(SearchStatus::Unavailable, std::memory_order_release);
    return false;
  }
  return true;
}

SearchStatus stationSearchStatus() {
  return state.load(std::memory_order_acquire);
}

int stationResultCount() {
  return stationSearchStatus() == SearchStatus::Complete ? resultCount : 0;
}

const SearchStation& stationResult(int index) {
  return results[index];
}
