#include <Arduino.h>
#include <Preferences.h>

#include <Adafruit_ST7789.h>
#include <Audio.h>
#include <SPI.h>
#include <WebServer.h>
#include <WiFi.h>
#include <atomic>

#include "Touch_CST3530.h"
#include "StationDirectory.h"
#include "BbcRewind.h"
#include "Orientation.h"

namespace {
constexpr int Backlight = 5;
constexpr uint32_t BacklightIdleMs = 5 * 60 * 1000;
constexpr uint16_t FullBacklightDuty = 1023;
constexpr uint8_t DimLevels[] = {10, 25, 50, 75, 100};
constexpr uint8_t DefaultDimIndex = 1;
constexpr int SetupRowStart = 124;
constexpr int SetupRowStep = 36;
constexpr int SetupRowHeight = 32;
constexpr uint16_t Background = 0x0842;
constexpr uint16_t Panel = 0x19C9;
constexpr uint16_t Accent = 0xF5C8;
constexpr uint16_t White = ST77XX_WHITE;

struct Station {
  const char* name;
  const char* url;
};

const Station stations[] = {
    {"BBC Radio 4", "https://as-hls-ww-live.akamaized.net/pool_55057080/live/ww/bbc_radio_fourfm/bbc_radio_fourfm.isml/bbc_radio_fourfm-audio%3d96000.norewind.m3u8"},
    {"BBC Radio 1", "https://as-hls-ww-live.akamaized.net/pool_01505109/live/ww/bbc_radio_one/bbc_radio_one.isml/bbc_radio_one-audio%3d96000.norewind.m3u8"},
    {"BBC Radio 2", "https://as-hls-ww-live.akamaized.net/pool_74208725/live/ww/bbc_radio_two/bbc_radio_two.isml/bbc_radio_two-audio%3d96000.norewind.m3u8"},
    {"BBC Radio 3", "https://as-hls-ww-live.akamaized.net/pool_23461179/live/ww/bbc_radio_three/bbc_radio_three.isml/bbc_radio_three-audio%3d96000.norewind.m3u8"},
    {"BBC Radio 4 Extra", "https://as-hls-ww-live.akamaized.net/pool_26173715/live/ww/bbc_radio_four_extra/bbc_radio_four_extra.isml/bbc_radio_four_extra-audio%3d96000.norewind.m3u8"},
    {"BBC Radio 5 Live", "https://as-hls-ww-live.akamaized.net/pool_89021708/live/ww/bbc_radio_five_live/bbc_radio_five_live.isml/bbc_radio_five_live-audio%3d96000.norewind.m3u8"},
    {"BBC Radio 6 Music", "https://as-hls-ww-live.akamaized.net/pool_81827798/live/ww/bbc_6music/bbc_6music.isml/bbc_6music-audio%3d96000.norewind.m3u8"},
    {"BBC Radio 1Xtra", "https://as-hls-ww-live.akamaized.net/pool_92079267/live/ww/bbc_1xtra/bbc_1xtra.isml/bbc_1xtra-audio%3d96000.norewind.m3u8"},
    {"BBC Asian Network", "https://as-hls-ww-live.akamaized.net/pool_22108647/live/ww/bbc_asian_network/bbc_asian_network.isml/bbc_asian_network-audio%3d96000.norewind.m3u8"},
    {"BBC Radio Wales", "https://as-hls-ww-live.akamaized.net/pool_97517794/live/ww/bbc_radio_wales_fm/bbc_radio_wales_fm.isml/bbc_radio_wales_fm-audio%3d96000.norewind.m3u8"},
    {"BBC Radio Ulster", "https://as-hls-ww-live.akamaized.net/pool_31244774/live/ww/bbc_radio_ulster/bbc_radio_ulster.isml/bbc_radio_ulster-audio%3d96000.norewind.m3u8"},
    {"BBC World Service", "https://as-hls-ww-live.akamaized.net/pool_87948813/live/ww/bbc_world_service/bbc_world_service.isml/bbc_world_service-audio%3d96000.norewind.m3u8"},
};
constexpr int StationCount = sizeof(stations) / sizeof(stations[0]);
constexpr int Rows = 4;
constexpr int RowStart = 68;
constexpr int RowStep = 36;
constexpr int RowHeight = 32;
constexpr int PageY = 214;
constexpr int FooterY = 273;
constexpr int KeyY = 108;
constexpr char Keys[][11] = {"1234567890", "QWERTYUIOP", "ASDFGHJKL-", "ZXCVBNM.'"};
enum class View { Presets, Keyboard, Results, Rewind, Setup, Orientation };
constexpr uint8_t AutoRotation = 0xFF;
constexpr int OrientationTop = 68;
constexpr int OrientationStep = 38;
constexpr int OrientationHeight = 32;
struct OrientationChoice {
  uint8_t value;
  const char* label;
};
constexpr OrientationChoice OrientationChoices[] = {
    {AutoRotation, "Auto"},
    {static_cast<uint8_t>(DisplayRotation::Portrait), "Portrait up"},
    {static_cast<uint8_t>(DisplayRotation::Inverted), "Portrait down"},
    {static_cast<uint8_t>(DisplayRotation::Left), "Landscape up"},
    {static_cast<uint8_t>(DisplayRotation::Right), "Landscape down"},
};

Adafruit_ST7789 screen(&SPI, 42, 41, 39);
Audio audio;
WebServer web(80);
Preferences preferences;
OrientationSensor orientationSensor;
DisplayRotation rotation = oppositeRotation(DisplayRotation::Portrait);
bool autoRotate = true;
bool orientationReady;
struct WifiCredentials {
  char ssid[33];
  char password[65];
};
WifiCredentials pendingCredentials{};
std::atomic<bool> decodedAudio{false};
bool touchReady;
bool audioReady;
bool pressed;
bool provisioning;
bool previousConnection;
bool playing;
bool heardAudio;
bool dirty = true;
SearchStation selected{stations[0].name, stations[0].url};
View view = View::Presets;
View rewindBack = View::Presets;
unsigned chosenDelay = 30;
unsigned rewindDelay;
unsigned rewindWindow;
bool rewindQueued;
bool rewindLoading;
bool rewindPlaying;
int presetPage;
int resultPage;
String query;
const char* searchHint = "Type a station name";
SearchStatus previousSearchStatus = SearchStatus::Idle;
String setupKey;
bool credentialsPending;
bool saveCredentials;
bool setupAttemptFailed;
uint32_t connectionStarted;
uint32_t setupClosesAt;
uint32_t lastDecodedAudio;
uint32_t lastTouch;
uint32_t lastTouchActivity;
uint8_t dimIndex = DefaultDimIndex;
bool backlightDimmed;
bool backlightPwm;
bool monoAudio;
const char* playbackStatus = "Ready";

int screenX(int x) { return x * screen.width() / 240; }
int screenY(int y) { return y * screen.height() / 320; }

void panel(int x, int y, int width, int height, int radius, uint16_t color) {
  screen.fillRoundRect(screenX(x), screenY(y), screenX(x + width) - screenX(x),
                       screenY(y + height) - screenY(y), radius, color);
}

void label(int x, int y, const char* value, uint16_t color, int size) {
  screen.setCursor(screenX(x), screenY(y));
  screen.setTextColor(color);
  screen.setTextSize(size);
  screen.print(value);
}

void button(int x, int y, int width, int height, const char* name, bool active) {
  panel(x, y, width, height, 6, active ? Accent : Panel);
  const int widthPx = screenX(x + width) - screenX(x);
  const int size = strlen(name) * 12 + 14 <= widthPx ? 2 : 1;
  char clipped[40];
  snprintf(clipped, sizeof(clipped), "%.*s", (widthPx - 12) / (6 * size), name);
  const int textY = y + (height - 7 * size * 320 / screen.height()) / 2;
  label(x + 6, textY, clipped, active ? Background : White, size);
}

int pageCount(int count) {
  return (count + Rows - 1) / Rows;
}

void drawKeyboard() {
  label(12, 50, searchHint, White, 1);
  panel(12, 68, 216, 30, 6, Panel);
  const int start = max(0, static_cast<int>(query.length()) - 17);
  label(18, 76, query.c_str() + start, Accent, 2);
  for (int row = 0; row < 4; ++row) {
    for (int col = 0; Keys[row][col]; ++col) {
      const char key[] = {Keys[row][col], '\0'};
      const int x = 12 + col * 22;
      const int y = KeyY + row * 29;
      panel(x, y, 20, 26, 3, Panel);
      label(x + 4, y + (isLandscape(rotation) ? 4 : 6), key, White, 2);
    }
  }
  button(12, 227, 128, 36, "Space", false);
  button(146, 227, 82, 36, "Delete", false);
  button(12, FooterY, 98, 36, "Back", false);
  button(124, FooterY, 104, 36, "Find", true);
}

void drawStationList() {
  const bool presets = view == View::Presets;
  const int count = presets ? StationCount : stationResultCount();
  const int page = presets ? presetPage : resultPage;
  const char* heading = "BBC presets";
  if (!presets) {
    switch (stationSearchStatus()) {
      case SearchStatus::Running: heading = "Searching Radio Browser..."; break;
      case SearchStatus::ClockUnavailable: heading = "Clock sync pending. Try again."; break;
      case SearchStatus::Unavailable: heading = "Directory unavailable"; break;
      default: heading = count ? "Search results" : "No playable stations found"; break;
    }
  }
  label(12, 50, heading, White, 1);

  const bool wide = isLandscape(rotation);
  for (int row = 0; row < Rows; ++row) {
    const int index = page * Rows + row;
    if (index >= count) break;
    const char* name = presets ? stations[index].name : stationResult(index).name.c_str();
    const char* url = presets ? stations[index].url : stationResult(index).url.c_str();
    const int x = wide ? 12 + (row % 2) * 110 : 12;
    const int y = wide ? RowStart + (row / 2) * 66 : RowStart + row * RowStep;
    button(x, y, wide ? 106 : 216, wide ? 54 : RowHeight, name, selected.url == url);
  }
  if (pageCount(count) > 1) {
    if (page > 0) button(12, PageY, 56, 28, "<", false);
    if (page + 1 < pageCount(count)) button(172, PageY, 56, 28, ">", false);
    char indicator[16];
    snprintf(indicator, sizeof(indicator), "%d / %d", page + 1, pageCount(count));
    if (BbcRewind::mediaUrl(selected.url).isEmpty()) label(99, PageY + 10, indicator, White, 1);
    else label(172, 50, indicator, White, 1);
  }
  if (!BbcRewind::mediaUrl(selected.url).isEmpty())
    button(74, PageY, 92, 28, "Rewind", false);
  char status[38];
  snprintf(status, sizeof(status), "%s | %.24s", playbackStatus, selected.name.c_str());
  label(12, 253, status, White, 1);
  button(12, FooterY, 68, 36, (playing || rewindQueued || rewindLoading) ? "Stop" : "Play", true);
  button(86, FooterY, 68, 36, presets ? "Search" : "Edit", false);
  button(160, FooterY, 68, 36, presets ? "Setup" : "BBC", false);
}

void drawSetup() {
  label(12, 52, "Setup", White, 2);
  label(12, 93, WiFi.isConnected() ? "Wi-Fi connected" : "Wi-Fi disconnected", White, 1);
  if (WiFi.isConnected()) label(12, 110, WiFi.SSID().substring(0, 32).c_str(), Accent, 1);
  button(12, SetupRowStart, 216, SetupRowHeight, "Change Wi-Fi", false);
  button(12, SetupRowStart + SetupRowStep, 216, SetupRowHeight, "Orientation", false);
  button(12, SetupRowStart + 2 * SetupRowStep, 216, SetupRowHeight,
         monoAudio ? "Audio: Mono" : "Audio: Stereo", false);
  char dimLabel[24];
  snprintf(dimLabel, sizeof(dimLabel), "Dim level: %u%%", DimLevels[dimIndex]);
  button(12, SetupRowStart + 3 * SetupRowStep, 216, SetupRowHeight, dimLabel, false);
  button(12, FooterY, 98, 36, "Back", false);
}

void drawOrientation() {
  label(12, 48, "Orientation", White, 1);
  const uint8_t current = autoRotate ? AutoRotation : static_cast<uint8_t>(oppositeRotation(rotation));
  for (size_t i = 0; i < sizeof(OrientationChoices) / sizeof(OrientationChoices[0]); ++i) {
    const auto& choice = OrientationChoices[i];
    const char* name = i == 0 && !orientationReady ? "Auto unavailable" : choice.label;
    button(12, OrientationTop + i * OrientationStep, 216, OrientationHeight,
           name, current == choice.value);
  }
  button(12, FooterY, 98, 36, "Back", false);
}

void drawRewind() {
  label(12, 51, selected.name.c_str(), White, 1);
  label(12, 76, "Time behind live", White, 2);
  char offset[24];
  snprintf(offset, sizeof(offset), "%u h %02u min", chosenDelay / 60, chosenDelay % 60);
  label(12, 107, offset, Accent, 3);
  button(12, 148, 48, 42, "-30", false);
  button(68, 148, 48, 42, "-5", false);
  button(124, 148, 48, 42, "+5", false);
  button(180, 148, 48, 42, "+30", false);
  label(12, 208, playbackStatus, White, 1);
  if (rewindWindow) {
    char available[32];
    snprintf(available, sizeof(available), "BBC window: %u min", rewindWindow);
    label(12, 229, available, White, 1);
  }
  button(12, FooterY, 68, 36, "Back", false);
  button(86, FooterY, 68, 36, "Live", false);
  button(160, FooterY, 68, 36, "Listen", true);
}

void draw() {
  screen.fillScreen(Background);
  label(12, 12, "RadioKing", Accent, 3);

  if (provisioning) {
    label(12, 58, "Wi-Fi setup", White, 2);
    label(12, 92, "Join RadioKing-Setup", White, 1);
    label(12, 116, "Setup password", White, 1);
    label(12, 132, setupKey.c_str(), Accent, 2);
    label(12, 166, "Open http://192.168.4.1", White, 1);
    label(12, 194, setupClosesAt ? "Connected. Closing setup..." :
        connectionStarted ? "Connecting to your network..." :
        setupAttemptFailed ? "Connection failed. Try again." :
        "Enter your Wi-Fi details there.", White, 1);
    if (WiFi.isConnected() && !setupClosesAt) button(12, 267, 98, 42, "Back", false);
    return;
  }

  if (view == View::Keyboard) drawKeyboard();
  else if (view == View::Rewind) drawRewind();
  else if (view == View::Setup) drawSetup();
  else if (view == View::Orientation) drawOrientation();
  else drawStationList();
}

void clearRewind() {
  BbcRewind::stop();
  rewindQueued = false;
  rewindLoading = false;
  rewindPlaying = false;
  rewindDelay = 0;
}

void stopPlayback(const char* status) {
  audio.stopSong();
  clearRewind();
  playing = false;
  heardAudio = false;
  playbackStatus = status;
  dirty = true;
  Serial.printf("Playback: %s\n", status);
}

void playStream(const char* url) {
  if (!WiFi.isConnected() || !audioReady) {
    stopPlayback(audioReady ? "Wi-Fi needed" : "Audio unavailable");
    return;
  }
  if (playing) audio.stopSong();
  decodedAudio.store(false, std::memory_order_relaxed);
  heardAudio = false;
  playbackStatus = "Buffering";
  dirty = false;
  draw();
  Serial.printf("Tuning: %s\n", selected.name.c_str());
  if (!audio.connecttohost(url)) {
    stopPlayback("Stream failed");
    return;
  }
  lastDecodedAudio = millis();
  playing = true;
  dirty = true;
}

void play() {
  clearRewind();
  playStream(selected.url.c_str());
}

void requestRewind() {
  stopPlayback("Finding BBC history");
  rewindDelay = chosenDelay;
  rewindQueued = true;
}

void updateRewind() {
  if (rewindQueued && !BbcRewind::running()) {
    rewindQueued = false;
    rewindLoading = BbcRewind::begin(BbcRewind::mediaUrl(selected.url), rewindDelay);
    if (!rewindLoading) stopPlayback("Rewind unavailable");
  }
  const auto state = BbcRewind::status();
  if (rewindLoading && state == BbcRewind::Status::Ready) {
    rewindLoading = false;
    rewindWindow = BbcRewind::windowMinutes();
    rewindPlaying = true;
    playStream(BbcRewind::PlaybackUrl);
  } else if ((rewindLoading || rewindPlaying) &&
             (state == BbcRewind::Status::OutsideWindow || state == BbcRewind::Status::Unavailable)) {
    rewindWindow = BbcRewind::windowMinutes();
    stopPlayback(state == BbcRewind::Status::OutsideWindow ? "Outside BBC window" : "Rewind unavailable");
  }
}

void startSetup() {
  if (provisioning) return;
  if (playing || rewindQueued || rewindLoading) stopPlayback("Stopped");
  char key[9];
  snprintf(key, sizeof(key), "%08lx", static_cast<unsigned long>(esp_random()));
  setupKey = key;
  WiFi.mode(WIFI_AP_STA);
  if (!WiFi.softAP("RadioKing-Setup", setupKey.c_str())) {
    playbackStatus = "Wi-Fi setup failed";
    dirty = true;
    return;
  }
  web.begin();
  provisioning = true;
  setupAttemptFailed = false;
  setupClosesAt = 0;
  dirty = true;
  Serial.println("Wi-Fi setup access point started");
}

void closeSetup() {
  web.stop();
  WiFi.softAPdisconnect(true);
  WiFi.mode(WIFI_STA);
  provisioning = false;
  setupKey = "";
  setupClosesAt = 0;
  dirty = true;
}

void configureSetup() {
  web.on("/", HTTP_GET, [] {
    if (web.client().localIP() != WiFi.softAPIP()) {
      web.send(403);
      return;
    }
    web.send(200, "text/html", R"HTML(<!doctype html><html lang="en"><meta name="viewport" content="width=device-width, initial-scale=1"><title>RadioKing Wi-Fi</title><h1>RadioKing Wi-Fi</h1><form method="post" action="/connect"><p><label>Network name <input name="ssid" required maxlength="32"></label></p><p><label>Password <input name="password" type="password" maxlength="64"></label></p><button>Connect</button></form></html>)HTML");
  });
  web.on("/connect", HTTP_POST, [] {
    if (web.client().localIP() != WiFi.softAPIP()) {
      web.send(403);
      return;
    }
    const String ssid = web.arg("ssid");
    const String password = web.arg("password");
    if (ssid.isEmpty() || ssid.length() > 32 || password.length() > 64) {
      web.send(400, "text/plain", "Check the network name and password length.");
      return;
    }
    ssid.toCharArray(pendingCredentials.ssid, sizeof(pendingCredentials.ssid));
    password.toCharArray(pendingCredentials.password, sizeof(pendingCredentials.password));
    credentialsPending = true;
    web.send(200, "text/plain", "Connecting. Check the RadioKing screen. If it stays in setup, return to the form and try again.");
  });
}

void updateNetwork() {
  if (provisioning) web.handleClient();
  if (credentialsPending) {
    WiFi.disconnect();
    WiFi.begin(pendingCredentials.ssid, pendingCredentials.password);
    credentialsPending = false;
    saveCredentials = true;
    setupAttemptFailed = false;
    connectionStarted = millis();
    setupClosesAt = 0;
    dirty = true;
  }

  const bool connected = WiFi.isConnected();
  if (connected != previousConnection) {
    previousConnection = connected;
    dirty = true;
    Serial.println(connected ? "Wi-Fi connected" : "Wi-Fi disconnected");
    if (connected) configTime(0, 0, "pool.ntp.org", "time.cloudflare.com");
    if (!connected && (playing || rewindQueued || rewindLoading)) stopPlayback("Wi-Fi lost");
  }
  if (connected && connectionStarted) {
    if (saveCredentials) {
      if (preferences.putBytes("wifi", &pendingCredentials, sizeof(pendingCredentials))
          != sizeof(pendingCredentials)) {
        Serial.println("Wi-Fi connected, but credentials could not be saved");
      }
      pendingCredentials = {};
      saveCredentials = false;
    }
    connectionStarted = 0;
    if (provisioning) setupClosesAt = millis() + 3000;
  }
  if (connectionStarted && millis() - connectionStarted > 20000) {
    connectionStarted = 0;
    pendingCredentials = {};
    saveCredentials = false;
    startSetup();
    setupAttemptFailed = true;
    dirty = true;
  }
  if (setupClosesAt && static_cast<int32_t>(millis() - setupClosesAt) >= 0) {
    setupClosesAt = 0;
    if (connected) closeSetup();
    dirty = true;
  }
}

void touchKeyboard(int x, int y) {
  if (y >= KeyY && y < KeyY + 4 * 29 && (y - KeyY) % 29 < 26) {
    const int row = (y - KeyY) / 29;
    const int col = (x - 12) / 22;
    if ((x - 12) % 22 < 20 && col < static_cast<int>(strlen(Keys[row])) &&
        query.length() < 48) query += Keys[row][col];
  } else if (y >= 227 && y < 263) {
    if (x < 140 && query.length() < 48) query += ' ';
    if (x >= 146 && !query.isEmpty()) query.remove(query.length() - 1);
  } else if (y >= FooterY && y < FooterY + 36) {
    if (x < 110) {
      view = View::Presets;
    } else if (x >= 124) {
      query.trim();
      if (query.length() < 2) {
        searchHint = "Enter at least 2 characters";
        dirty = true;
        return;
      }
      if (!WiFi.isConnected() || stationSearchStatus() == SearchStatus::Running) {
        searchHint = WiFi.isConnected() ? "Previous search still running" : "Wi-Fi required";
        dirty = true;
        return;
      }
      startStationSearch(query);
      resultPage = 0;
      view = View::Results;
    }
  } else {
    return;
  }
  searchHint = "Type a station name";
  dirty = true;
}

void touchStationList(int x, int y) {
  const bool presets = view == View::Presets;
  int& page = presets ? presetPage : resultPage;
  const int count = presets ? StationCount : stationResultCount();
  const bool wide = isLandscape(rotation);
  const int step = wide ? 66 : RowStep;
  const int height = wide ? 54 : RowHeight;
  const int rows = wide ? 2 : Rows;
  if (y >= RowStart && y < RowStart + rows * step && (y - RowStart) % step < height) {
    const int row = (y - RowStart) / step;
    if (wide && x >= 118 && x < 122) return;
    const int index = page * Rows + (wide ? row * 2 + (x >= 122) : row);
    if (index >= count) return;
    SearchStation next = presets ? SearchStation{stations[index].name, stations[index].url}
                                 : stationResult(index);
    if (selected.url == next.url) return;
    selected = next;
    rewindWindow = 0;
    if (playing) play();
    else if (rewindQueued || rewindLoading) stopPlayback("Ready");
  } else if (y >= PageY && y < PageY + 28) {
    if (x < 68 && page > 0) --page;
    if (x >= 172 && page + 1 < pageCount(count)) ++page;
    if (x >= 74 && x < 166 && !BbcRewind::mediaUrl(selected.url).isEmpty()) {
      rewindBack = view;
      view = View::Rewind;
      if (rewindPlaying) chosenDelay = rewindDelay;
    }
  } else if (y >= FooterY && y < FooterY + 36) {
    if (x < 80) {
      if (playing || rewindQueued || rewindLoading) stopPlayback("Stopped");
      else play();
    } else if (x >= 86 && x < 154) {
      view = View::Keyboard;
      searchHint = "Type a station name";
    } else if (x >= 160) {
      if (presets) view = View::Setup;
      else view = View::Presets;
    }
  } else {
    return;
  }
  dirty = true;
}

void touchRewind(int x, int y) {
  if (y >= 148 && y < 190) {
    int step = 0;
    if (x < 60) step = -30;
    else if (x >= 68 && x < 116) step = -5;
    else if (x >= 124 && x < 172) step = 5;
    else if (x >= 180) step = 30;
    chosenDelay = constrain(static_cast<int>(chosenDelay) + step, 5, 360);
  } else if (y >= FooterY && y < FooterY + 36) {
    if (x < 80) view = rewindBack;
    else if (x >= 86 && x < 154) {
      view = rewindBack;
      play();
    } else if (x >= 160) requestRewind();
  } else {
    return;
  }
  dirty = true;
}

void touchSetup(int x, int y) {
  if (x < 110 && y >= FooterY && y < FooterY + 36) {
    view = View::Presets;
  } else if (y >= SetupRowStart && y < SetupRowStart + 4 * SetupRowStep &&
             (y - SetupRowStart) % SetupRowStep < SetupRowHeight) {
    switch ((y - SetupRowStart) / SetupRowStep) {
      case 0: startSetup(); break;
      case 1: view = View::Orientation; break;
      case 2:
        monoAudio = !monoAudio;
        audio.forceMono(monoAudio);
        preferences.putBool("mono", monoAudio);
        break;
      case 3:
        dimIndex = (dimIndex + 1) % sizeof(DimLevels);
        preferences.putUChar("dimLevel", dimIndex);
        break;
    }
  } else {
    return;
  }
  dirty = true;
}

void setDisplayRotation(DisplayRotation next) {
  if (rotation == next) return;
  rotation = next;
  screen.setRotation(static_cast<uint8_t>(rotation));
  Serial.printf("Display rotation: %u (%d x %d)\n", static_cast<unsigned>(rotation),
                screen.width(), screen.height());
  dirty = true;
}

void touchOrientation(int x, int y) {
  if (x < 110 && y >= FooterY && y < FooterY + 36) {
    view = View::Setup;
    dirty = true;
    return;
  }
  if (y < OrientationTop) return;
  const int offset = y - OrientationTop;
  const size_t index = offset / OrientationStep;
  if (index >= sizeof(OrientationChoices) / sizeof(OrientationChoices[0]) ||
      offset % OrientationStep >= OrientationHeight ||
      (index == 0 && !orientationReady)) return;

  const uint8_t value = OrientationChoices[index].value;
  autoRotate = value == AutoRotation;
  preferences.putUChar("rotation", value);
  if (!autoRotate) setDisplayRotation(oppositeRotation(static_cast<DisplayRotation>(value)));
  dirty = true;
}

void setBacklightDimmed(bool dimmed) {
  dimmed = dimmed && DimLevels[dimIndex] < 100;
  if (!backlightPwm || dimmed == backlightDimmed) return;
  backlightDimmed = dimmed;
  const unsigned percent = dimmed ? DimLevels[dimIndex] : 100;
  ledcWrite(Backlight, FullBacklightDuty * percent / 100);
  Serial.printf("Backlight: %u%%\n", percent);
}

void updateTouch() {
  if (!touchReady || millis() - lastTouch < 40) return;
  lastTouch = millis();
  if (!Touch2_Read_Data()) return;
  uint16_t rawX, rawY;
  uint8_t count;
  const bool down = Touch2_Get_XY(&rawX, &rawY, nullptr, &count, 1);
  const bool tapped = down && !pressed;
  pressed = down;
  if (down) {
    lastTouchActivity = lastTouch;
    if (backlightDimmed) {
      setBacklightDimmed(false);
      return;
    }
  }
  if (!tapped || rawX >= 240 || rawY >= 320) return;
  int x = rawX;
  int y = rawY;
  rotateTouch(rotation, x, y);
  x = ((x + 1) * 240 - 1) / screen.width();
  y = ((y + 1) * 320 - 1) / screen.height();
  if (x < 12 || x >= 228) return;

  if (provisioning) {
    if (WiFi.isConnected() && !setupClosesAt && x < 110 && y >= 267 && y < 309)
      closeSetup();
    return;
  }

  if (view == View::Keyboard) touchKeyboard(x, y);
  else if (view == View::Rewind) touchRewind(x, y);
  else if (view == View::Setup) touchSetup(x, y);
  else if (view == View::Orientation) touchOrientation(x, y);
  else touchStationList(x, y);
}

void updateOrientation() {
  if (!autoRotate || pressed) return;
  setDisplayRotation(orientationSensor.update(rotation, millis()));
}
}

void audio_process_extern(int16_t*, uint16_t samples, bool* continueI2S) {
  *continueI2S = true;
  if (samples) decodedAudio.store(true, std::memory_order_relaxed);
}

void setup() {
  Serial.begin(115200);
  pinMode(Backlight, OUTPUT);
  digitalWrite(Backlight, LOW);
  SPI.begin(40, -1, 45, 42);
  screen.init(240, 320);
  screen.setRotation(static_cast<uint8_t>(rotation));
  screen.setTextWrap(false);
  backlightPwm = ledcAttach(Backlight, 20000, 10);
  if (backlightPwm) ledcWrite(Backlight, FullBacklightDuty);
  else digitalWrite(Backlight, HIGH);
  lastTouchActivity = millis();

  touchReady = TOUCH2_Init();
  orientationReady = orientationSensor.begin();
  audioReady = audio.setPinout(48, 38, 47);
  audio.setVolume(12);
  audio.setConnectionTimeout(3500, 6000);
  Serial.printf("RadioKing ready. CST3530=%s I2S=%s PSRAM=%u\n",
                touchReady ? "OK" : "FAILED", audioReady ? "OK" : "FAILED",
                static_cast<unsigned>(ESP.getPsramSize()));
  configureSetup();
  preferences.begin("radioking");
  monoAudio = preferences.getBool("mono", false);
  audio.forceMono(monoAudio);
  dimIndex = preferences.getUChar("dimLevel", DefaultDimIndex);
  if (dimIndex >= sizeof(DimLevels)) dimIndex = DefaultDimIndex;
  Serial.printf("Audio mode: %s; backlight PWM=%s; dim level=%u%%\n",
                monoAudio ? "mono" : "stereo", backlightPwm ? "OK" : "unavailable", DimLevels[dimIndex]);
  const uint8_t savedRotation = preferences.getUChar("rotation", AutoRotation);
  autoRotate = savedRotation > 3;
  if (!autoRotate) {
    setDisplayRotation(oppositeRotation(static_cast<DisplayRotation>(savedRotation)));
  }
  Serial.printf("Orientation QMI8658=%s; rotation=%s\n",
                orientationReady ? "OK" : "unavailable", autoRotate ? "auto" : "locked");
  WiFi.persistent(false);
  WiFi.mode(WIFI_STA);
  WiFi.setAutoReconnect(true);
  WifiCredentials saved{};
  if (preferences.getBytesLength("wifi") != sizeof(saved) ||
      preferences.getBytes("wifi", &saved, sizeof(saved)) != sizeof(saved) ||
      saved.ssid[0] == '\0') startSetup();
  else {
    WiFi.begin(saved.ssid, saved.password);
    connectionStarted = millis();
  }
  draw();
  dirty = false;
  Serial.printf("RadioKing screen ready. Wi-Fi setup=%s\n", provisioning ? "active" : "inactive");
}

void loop() {
  updateNetwork();
  updateRewind();
  const SearchStatus searchStatus = stationSearchStatus();
  if (searchStatus != previousSearchStatus) {
    previousSearchStatus = searchStatus;
    if (view == View::Results) dirty = true;
  }
  updateTouch();
  if (millis() - lastTouchActivity >= BacklightIdleMs) setBacklightDimmed(true);
  updateOrientation();
  if (playing) {
    audio.loop();
    if (decodedAudio.exchange(false, std::memory_order_relaxed)) {
      lastDecodedAudio = millis();
      if (!heardAudio) {
        playbackStatus = rewindPlaying ? "Playing rewind" : "Playing";
        heardAudio = true;
        dirty = true;
        Serial.println("Playback: PCM decoded");
      }
    }
    if (!audio.isRunning() || millis() - lastDecodedAudio > 20000) {
      stopPlayback(heardAudio ? "Stream ended" : "Stream failed");
    }
  }
  if (dirty) {
    draw();
    dirty = false;
  }
  delay(2);
}
