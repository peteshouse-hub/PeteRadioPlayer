#pragma once

#include <Arduino.h>

namespace PlaybackEngine {
enum class Outcome { ConnectionFailed, Ended };

struct Event {
  unsigned generation;
  Outcome outcome;
};

struct Title {
  unsigned generation;
  char text[257];
};

bool begin(bool mono);
void setMono(bool mono);
bool play(const char* url, unsigned generation);
void stop(unsigned generation);
bool takeEvent(Event& event);
bool takeTitle(Title& title);
unsigned takeDecodedGeneration();
}
