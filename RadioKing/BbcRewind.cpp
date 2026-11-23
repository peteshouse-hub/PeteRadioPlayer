#include "BbcRewind.h"

#include <HTTPClient.h>
#include <NetworkClientSecure.h>
#include <NetworkServer.h>
#include <atomic>
#include <esp_heap_caps.h>
#include <memory>
#include <time.h>

#include "BbcTrust.h"

namespace BbcRewind {
namespace {
constexpr size_t MaxManifest = 768 * 1024;
constexpr unsigned Batch = 4;
constexpr unsigned Overlap = 2;

struct Session {
  String url;
  unsigned minutes;
  unsigned generation;
};
std::atomic<unsigned> generation{0};
std::atomic<unsigned> window{0};
std::atomic<Status> state{Status::Idle};
std::atomic<bool> workerRunning{false};

class ManifestBuffer : public Stream {
public:
  std::unique_ptr<char, decltype(&free)> data{
      static_cast<char*>(heap_caps_malloc(MaxManifest, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT)), free};
  size_t length = 0;

  size_t write(const uint8_t* bytes, size_t size) override {
    if (!data || size > MaxManifest - 1 - length) return 0;
    memcpy(data.get() + length, bytes, size);
    length += size;
    data.get()[length] = '\0';
    return size;
  }

  size_t write(uint8_t byte) override { return write(&byte, 1); }
  int available() override { return 0; }
  int read() override { return -1; }
  int peek() override { return -1; }
};

struct Segment {
  const char* uri;
  size_t length;
  double duration;
};

class Playlist {
public:
  ManifestBuffer manifest;
  uint64_t first = 0;
  unsigned count = 0;
  unsigned target = 0;
  uint32_t durationMs = 0;
  String base;

  template <typename Visitor>
  bool visit(Visitor visitor) {
    const char* line = manifest.data.get();
    if (!line || strncmp(line, "#EXTM3U", 7) != 0) return false;
    double segmentTime = 0;
    unsigned index = 0;
    bool haveSequence = false;
    while (*line) {
      const char* end = strchr(line, '\n');
      if (!end) end = line + strlen(line);
      size_t size = end - line;
      if (size && line[size - 1] == '\r') --size;
      if (strncmp(line, "#EXT-X-MEDIA-SEQUENCE:", 22) == 0) {
        if (index) return false;
        first = strtoull(line + 22, nullptr, 10);
        haveSequence = true;
      } else if (strncmp(line, "#EXT-X-TARGETDURATION:", 22) == 0) {
        target = strtoul(line + 22, nullptr, 10);
      } else if (strncmp(line, "#EXT-X-KEY:", 11) == 0 ||
                 strncmp(line, "#EXT-X-BYTERANGE:", 17) == 0 ||
                 strncmp(line, "#EXT-X-MAP:", 11) == 0 ||
                 strncmp(line, "#EXT-X-GAP", 10) == 0 ||
                 strncmp(line, "#EXT-X-DISCONTINUITY", 20) == 0) {
        return false;
      } else if (strncmp(line, "#EXTINF:", 8) == 0) {
        if (segmentTime > 0) return false;
        segmentTime = strtod(line + 8, nullptr);
        if (!(segmentTime > 0 && segmentTime <= 60)) return false;
      } else if (size && *line != '#') {
        if (!haveSequence || !segmentTime || size > 380 || !target ||
            size < 4 || memcmp(line + size - 3, ".ts", 3) != 0) return false;
        for (size_t i = 0; i < size; ++i) {
          const unsigned char ch = line[i];
          if (!isalnum(ch) && ch != '-' && ch != '_' && ch != '.' && ch != '=') return false;
        }
        char* afterSequence = nullptr;
        const char* number = line + size - 4;
        while (number >= line && isdigit(static_cast<unsigned char>(*number))) {
          if (number == line) break;
          --number;
        }
        if (*number != '-' || strtoull(number + 1, &afterSequence, 10) != first + index ||
            afterSequence != line + size - 3) return false;
        visitor(index++, Segment{line, size, segmentTime});
        segmentTime = 0;
      }
      line = *end ? end + 1 : end;
    }
    return count == index && count >= 3 && segmentTime == 0;
  }

  bool load(const String& url) {
    if (!manifest.data || time(nullptr) < 1700000000) return false;
    manifest.length = 0;
    NetworkClientSecure tls;
    tls.setCACert(BbcTrust);
    tls.setHandshakeTimeout(5);
    HTTPClient http;
    http.setConnectTimeout(4000);
    http.setTimeout(6000);
    if (!http.begin(tls, url) || http.GET() != HTTP_CODE_OK) return false;
    if (http.writeToStream(&manifest) <= 0) return false;
    base = url.substring(0, url.lastIndexOf('/') + 1);
    count = 0;
    durationMs = 0;
    return visit([&](unsigned, const Segment& segment) {
      ++count;
      durationMs += lround(segment.duration * 1000);
    });
  }

  uint64_t seek(uint32_t milliseconds) {
    unsigned selected = 0;
    uint32_t elapsed = 0;
    visit([&](unsigned index, const Segment& segment) {
      if (elapsed <= milliseconds) selected = index;
      elapsed += lround(segment.duration * 1000);
    });
    return first + selected;
  }

  String slice(uint64_t start, uint64_t end) {
    char header[112];
    snprintf(header, sizeof(header), "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:%u\n#EXT-X-MEDIA-SEQUENCE:%llu\n",
             target, static_cast<unsigned long long>(start));
    String body(header);
    if (!body.reserve(6000)) return {};
    if (!visit([&](unsigned index, const Segment& segment) {
      const uint64_t seq = first + index;
      if (seq < start || seq >= end) return;
      char info[32];
      snprintf(info, sizeof(info), "#EXTINF:%.3f,\n", segment.duration);
      body += info;
      body += base;
      body.concat(segment.uri, segment.length);
      body += '\n';
    })) return {};
    return body;
  }
};

bool active(const Session& session) {
  return generation.load() == session.generation;
}

void runSession(const Session& session) {
  Playlist playlist;
  const uint32_t started = millis();
  while (active(session) && time(nullptr) < 1700000000 && millis() - started < 6000) delay(100);
  if (!active(session)) return;
  if (!playlist.load(session.url)) {
    if (active(session)) state.store(Status::Unavailable);
    return;
  }
  if (!active(session)) return;
  window.store(playlist.durationMs / 60000);
  const uint32_t delayMs = session.minutes * 60000;
  if (delayMs > playlist.durationMs) {
    state.store(Status::OutsideWindow);
    return;
  }

  uint64_t next = playlist.seek(playlist.durationMs - delayMs);
  const uint64_t initial = next;
  NetworkServer server(IPAddress(127, 0, 0, 1), 8084, 1);
  server.begin();
  if (!server) {
    state.store(Status::Unavailable);
    return;
  }
  Serial.printf("BBC rewind: %u min, %u min available, sequence %llu\n",
                session.minutes, window.load(), static_cast<unsigned long long>(next));
  state.store(Status::Ready);
  NetworkClient client;
  while (active(session)) {
    if (!client.connected()) client = server.accept();
    if (!client.available()) {
      delay(5);
      continue;
    }
    client.setTimeout(350);
    const String request = client.readStringUntil('\n');
    if (!request.startsWith("GET /rewind.m3u8 HTTP/")) {
      client.print("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
      client.stop();
      continue;
    }
    for (int i = 0; i < 12; ++i) {
      const String header = client.readStringUntil('\n');
      if (header.isEmpty() || header == "\r") break;
    }
    if (next >= playlist.first + playlist.count &&
        (!playlist.load(session.url) || next < playlist.first || next >= playlist.first + playlist.count)) {
      client.print("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
      if (active(session)) state.store(Status::Unavailable);
      return;
    }
    if (!active(session)) return;
    uint64_t from = next == initial ? next : next - Overlap;
    if (from < playlist.first) from = playlist.first;
    const uint64_t end = std::min<uint64_t>(next + Batch, playlist.first + playlist.count);
    if (end - from < 3) from = end - 3;
    const String body = playlist.slice(from, end);
    if (body.isEmpty()) {
      state.store(Status::Unavailable);
      return;
    }
    client.printf("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apple.mpegurl\r\nContent-Length: %u\r\nConnection: keep-alive\r\n\r\n",
                  static_cast<unsigned>(body.length()));
    if (client.print(body) == body.length()) {
      Serial.printf("BBC rewind served sequence %llu\n", static_cast<unsigned long long>(next));
      next = end;
    }
  }
}

void task(void* arg) {
  {
    std::unique_ptr<Session> session(static_cast<Session*>(arg));
    runSession(*session);
  }
  workerRunning.store(false);
  vTaskDelete(nullptr);
}
}

String mediaUrl(const String& liveUrl) {
  if ((liveUrl.startsWith("https://a.files.bbci.co.uk/") || liveUrl.startsWith("http://a.files.bbci.co.uk/")) &&
      liveUrl.indexOf("/audio/simulcast/hls/") >= 0 && liveUrl.endsWith("/bbc_tees.m3u8")) {
    return "https://as-hls-ww-live.akamaized.net/pool_08918172/live/ww/bbc_tees/bbc_tees.isml/bbc_tees-audio%3d96000.m3u8";
  }
  if (!liveUrl.startsWith("https://as-hls-ww-live.akamaized.net/") &&
      !liveUrl.startsWith("http://as-hls-ww-live.akamaized.net/") &&
      !liveUrl.startsWith("https://as-hls-uk-live.akamaized.net/") &&
      !liveUrl.startsWith("http://as-hls-uk-live.akamaized.net/")) return {};
  const int stationStart = liveUrl.indexOf("/bbc_");
  if (stationStart < 0 || !liveUrl.endsWith(".m3u8")) return {};
  const int stationEnd = liveUrl.indexOf('/', stationStart + 1);
  if (stationEnd < 0) return {};
  const String station = liveUrl.substring(stationStart + 1, stationEnd);
  if (!liveUrl.substring(stationEnd).startsWith("/" + station + ".isml/" + station + "-audio")) return {};
  String url = liveUrl.startsWith("http://") ? "https://" + liveUrl.substring(7) : liveUrl;
  if (url.endsWith(".norewind.m3u8")) url.remove(url.length() - 14, 9);
  return url;
}

bool begin(const String& playlistUrl, unsigned minutes) {
  if (playlistUrl.isEmpty() || !minutes || workerRunning.exchange(true)) return false;
  window.store(0);
  state.store(Status::Loading);
  Session* session = new (std::nothrow) Session{playlistUrl, minutes, ++generation};
  if (!session || xTaskCreate(task, "BBC rewind", 14336, session, 1, nullptr) != pdPASS) {
    delete session;
    workerRunning.store(false);
    state.store(Status::Unavailable);
    return false;
  }
  return true;
}

void stop() {
  ++generation;
  state.store(Status::Idle);
}

Status status() { return state.load(); }
bool running() { return workerRunning.load(); }
unsigned windowMinutes() { return window.load(); }
}
