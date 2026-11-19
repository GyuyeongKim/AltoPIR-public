#ifndef REINSPIRE_BENCH_FIXTURE_H_
#define REINSPIRE_BENCH_FIXTURE_H_

#include <cstdint>
#include <vector>

namespace bench {
constexpr uint64_t kSeed = 0x416c746f504952ULL;
constexpr size_t kWords = 16384;

inline uint64_t Mix(uint64_t& state) {
  uint64_t z = (state += 0x9e3779b97f4a7c15ULL);
  z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
  z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
  return z ^ (z >> 31);
}

inline std::vector<uint16_t> Record(uint32_t index) {
  uint64_t state = kSeed ^ (0xd1b54a32d192ed03ULL * (uint64_t(index) + 1));
  std::vector<uint16_t> out(kWords);
  for (auto& x : out) x = static_cast<uint16_t>(Mix(state) % 65535);
  return out;
}

inline std::vector<uint32_t> Indices(unsigned trial, unsigned batch, uint32_t records) {
  uint64_t state = kSeed ^ 0x72657175657374ULL ^ trial;
  std::vector<uint32_t> out(batch);
  for (auto& x : out) x = uint32_t(Mix(state) & (records - 1));
  return out;
}
}  // namespace bench
#endif
