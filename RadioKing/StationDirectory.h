#pragma once

#include <Arduino.h>

struct SearchStation {
  String name;
  String url;
};

constexpr int MaxSearchResults = 40;

enum class SearchStatus { Idle, Running, Complete, Unavailable, ClockUnavailable };

bool startStationSearch(const String& query);
SearchStatus stationSearchStatus();
int stationResultCount();
const SearchStation& stationResult(int index);
