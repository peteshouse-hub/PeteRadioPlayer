#include "PlaybackEngine.h"

#include <Audio.h>
#include <atomic>
#include <freertos/queue.h>

namespace {
struct Command {
  unsigned generation;
  char url[1025];
};

Audio audio;
QueueHandle_t commands;
QueueHandle_t events;
QueueHandle_t titles;
std::atomic<bool> monoRequested{false};
std::atomic<unsigned> decodedGeneration{0};
unsigned activeGeneration;

void audioTask(void*) {
  bool playing = false;
  bool monoApplied = monoRequested.load();
  audio.forceMono(monoApplied);
  Command command{};
  for (;;) {
    if (xQueueReceive(commands, &command, playing ? 0 : pdMS_TO_TICKS(25)) == pdPASS) {
      audio.stopSong();
      activeGeneration = command.generation;
      playing = command.url[0] && audio.connecttohost(command.url);
      if (command.url[0] && !playing) {
        const PlaybackEngine::Event event{activeGeneration, PlaybackEngine::Outcome::ConnectionFailed};
        xQueueOverwrite(events, &event);
      }
    }
    const bool mono = monoRequested.load();
    if (monoApplied != mono) {
      audio.forceMono(mono);
      monoApplied = mono;
    }
    if (playing) {
      audio.loop();
      if (!audio.isRunning()) {
        playing = false;
        const PlaybackEngine::Event event{activeGeneration, PlaybackEngine::Outcome::Ended};
        xQueueOverwrite(events, &event);
      }
      // Let other tasks run even when the stream always has packets ready.
      vTaskDelay(1);
    }
  }
}
}

namespace PlaybackEngine {
bool begin(bool mono) {
  if (!audio.setPinout(48, 38, 47)) return false;
  audio.setVolume(12);
  audio.setConnectionTimeout(3500, 6000);
  audio.setBufsize(-1, 1024 * 1024);
  monoRequested.store(mono);
  commands = xQueueCreate(1, sizeof(Command));
  events = xQueueCreate(1, sizeof(Event));
  titles = xQueueCreate(1, sizeof(Title));
  if (commands && events && titles &&
      xTaskCreate(audioTask, "radio audio", 16384, nullptr, 4, nullptr) == pdPASS) {
    Serial.println("Playback task ready (priority 4, 1 MiB PSRAM buffer capacity)");
    return true;
  }
  if (commands) vQueueDelete(commands);
  if (events) vQueueDelete(events);
  if (titles) vQueueDelete(titles);
  commands = events = titles = nullptr;
  return false;
}

void setMono(bool mono) { monoRequested.store(mono); }

bool play(const char* url, unsigned generation) {
  Command command{generation, {}};
  if (!commands || !url || strlen(url) >= sizeof(command.url)) return false;
  strlcpy(command.url, url, sizeof(command.url));
  return xQueueOverwrite(commands, &command) == pdPASS;
}

void stop(unsigned generation) {
  if (!commands) return;
  const Command command{generation, {}};
  xQueueOverwrite(commands, &command);
}

bool takeEvent(Event& event) {
  return events && xQueueReceive(events, &event, 0) == pdPASS;
}

bool takeTitle(Title& title) {
  return titles && xQueueReceive(titles, &title, 0) == pdPASS;
}

unsigned takeDecodedGeneration() { return decodedGeneration.exchange(0, std::memory_order_relaxed); }
}

void audio_process_extern(int16_t*, uint16_t samples, bool* continueI2S) {
  *continueI2S = true;
  if (samples) decodedGeneration.store(activeGeneration, std::memory_order_relaxed);
}

void audio_showstreamtitle(const char* text) {
  if (!titles) return;
  PlaybackEngine::Title update{activeGeneration, {}};
  if (text) strlcpy(update.text, text, sizeof(update.text));
  xQueueOverwrite(titles, &update);
}
