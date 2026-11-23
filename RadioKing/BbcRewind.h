#pragma once

#include <Arduino.h>

namespace BbcRewind {
enum class Status { Idle, Loading, Ready, OutsideWindow, Unavailable };
constexpr char PlaybackUrl[] = "http://127.0.0.1:8084/rewind.m3u8";

String mediaUrl(const String& liveUrl);
bool begin(const String& playlistUrl, unsigned minutes);
void stop();
bool running();
Status status();
unsigned windowMinutes();
uint32_t startTimestamp();
}
