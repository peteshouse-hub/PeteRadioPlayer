#include "ProgrammeMetadata.h"

#include <ArduinoJson.h>
#include <HTTPClient.h>
#include <NetworkClientSecure.h>
#include <atomic>
#include <esp_heap_caps.h>
#include <memory>

#include "BbcRewind.h"
#include "BroadcastTime.h"
#include "ProgrammeTrust.h"

namespace ProgrammeMetadata {
namespace {
enum class State { Idle, Fetching, Ready };
std::atomic<State> state{State::Idle};
Programme result;
String requestedService;
time_t requestedTime;

class ScheduleBody : public Stream {
public:
  static constexpr size_t Capacity = 512 * 1024;
  std::unique_ptr<char, decltype(&free)> data{
      static_cast<char*>(heap_caps_malloc(Capacity, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT)), free};
  size_t used = 0;

  size_t write(const uint8_t* bytes, size_t count) override {
    if (!data || count >= Capacity - used) return 0;
    memcpy(data.get() + used, bytes, count);
    used += count;
    data.get()[used] = '\0';
    return count;
  }

  size_t write(uint8_t ch) override { return write(&ch, 1); }
  int available() override { return 0; }
  int read() override { return -1; }
  int peek() override { return -1; }
};

bool findProgramme(const String& service, time_t when, time_t scheduleDate) {
  tm utc;
  gmtime_r(&scheduleDate, &utc);
  char date[11];
  strftime(date, sizeof(date), "%F", &utc);
  const String url = "https://rms.api.bbc.co.uk/v2/broadcasts/schedules/" + service + "/" + date;
  NetworkClientSecure tls;
  tls.setCACert(ProgrammeTrust);
  tls.setHandshakeTimeout(5);
  HTTPClient http;
  http.setTimeout(5000);
  http.setConnectTimeout(3500);
  http.setUserAgent("RadioKing-ESP32/0.2");
  if (!http.begin(tls, url) || http.GET() != HTTP_CODE_OK) return false;
  ScheduleBody body;
  if (!body.data || http.getSize() >= static_cast<int>(ScheduleBody::Capacity) ||
      http.writeToStream(&body) <= 0) return false;

  JsonDocument filter;
  auto item = filter["data"][0];
  item["start"] = true;
  item["end"] = true;
  item["titles"]["primary"] = true;
  item["titles"]["secondary"] = true;
  JsonDocument schedule;
  if (deserializeJson(schedule, body.data.get(), DeserializationOption::Filter(filter))) return false;
  for (JsonObject entry : schedule["data"].as<JsonArray>()) {
    const time_t start = bbcUtcTime(entry["start"] | "");
    const time_t end = bbcUtcTime(entry["end"] | "");
    if (!start || when < start || when >= end) continue;
    String title = entry["titles"]["primary"] | "";
    String subtitle = entry["titles"]["secondary"] | "";
    if (title.isEmpty()) return false;
    if (!subtitle.isEmpty() && subtitle != title) title += " / " + subtitle;
    result.title = title;
    result.start = start;
    result.end = end;
    return true;
  }
  return false;
}

void lookupTask(void*) {
  if (!findProgramme(requestedService, requestedTime, requestedTime)) {
    // During British Summer Time the next schedule date starts at 23:00 UTC.
    const time_t secondsInDay = requestedTime % 86400;
    if (secondsInDay >= 23 * 3600)
      findProgramme(requestedService, requestedTime, requestedTime + 86400);
  }
  state.store(State::Ready);
  vTaskDelete(nullptr);
}
}

String serviceId(const String& stream) {
  const String playlist = BbcRewind::mediaUrl(stream);
  const int start = playlist.indexOf("/bbc_");
  if (start < 0) return {};
  const int end = playlist.indexOf('/', start + 1);
  if (end < 0) return {};
  const String service = playlist.substring(start + 1, end);
  for (unsigned char ch : service)
    if (!isalnum(ch) && ch != '_') return {};
  return service;
}

bool fetch(const String& service, time_t when, unsigned generation) {
  if (service.isEmpty() || when < 1700000000 || state.load() != State::Idle) return false;
  requestedService = service;
  requestedTime = when;
  result = {};
  result.generation = generation;
  state.store(State::Fetching);
  if (xTaskCreate(lookupTask, "BBC programme", 12288, nullptr, 1, nullptr) != pdPASS) {
    state.store(State::Idle);
    return false;
  }
  return true;
}

bool take(Programme& programme) {
  if (state.load() != State::Ready) return false;
  programme = std::move(result);
  state.store(State::Idle);
  return true;
}
}
