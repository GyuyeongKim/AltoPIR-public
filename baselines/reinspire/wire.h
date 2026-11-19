#ifndef REINSPIRE_BENCH_WIRE_H_
#define REINSPIRE_BENCH_WIRE_H_

#include <array>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "crypto/pir/pir_params.h"
#include "crypto/polynomial.h"

namespace bench {
namespace pir = private_membership::rlwe::v2;
using Request = pir::PirRequest<uint64_t>;
using Response = pir::PirResponse<uint64_t>;
using Poly = pir::Polynomial<uint64_t>;
using Bytes = std::vector<uint8_t>;
constexpr uint32_t kDegree = 2048, kShards = 8, kRecordBytes = 32768;
constexpr uint32_t kPlaintext = 65535, kQ1Bits = 29, kQ2Bits = 18;
constexpr uint64_t kMaxFrameBytes = 16 * 1024 * 1024;
constexpr size_t kHeaderBytes = 92;
enum class Kind : uint32_t { Setup = 0, Request = 1, Response = 2 };
constexpr uint32_t kPacked = 1;

inline void Require(bool condition, const char* message) {
  if (!condition) throw std::runtime_error(message);
}

template <typename T>
T Take(absl::StatusOr<T> value) {
  if (!value.ok()) throw std::runtime_error(std::string(value.status().message()));
  return std::move(*value);
}

inline uint64_t Mask(unsigned bits) { return (uint64_t{1} << bits) - 1; }

inline void Put(Bytes& out, uint64_t value, unsigned bytes) {
  for (unsigned i = 0; i < bytes; ++i) out.push_back(uint8_t(value >> (8 * i)));
}

struct Reader {
  const Bytes& data;
  size_t pos = 0;
  uint64_t Get(unsigned bytes) {
    Require(bytes <= 8 && pos <= data.size() && bytes <= data.size() - pos, "truncated field");
    uint64_t value = 0;
    for (unsigned i = 0; i < bytes; ++i) value |= uint64_t(data[pos++]) << (8 * i);
    return value;
  }
  void End() const { Require(pos == data.size(), "trailing bytes"); }
};

struct Profile { uint32_t records = 0, q_bits = 0, t = 0; };

inline void CheckProfile(const Profile& p) {
  Require(p.records >= 64 && p.records <= 2097152 && (p.records & (p.records - 1)) == 0, "records must be a power of two from 64 to 2097152");
  Require((p.q_bits == 52 || p.q_bits == 54) && (p.t == 1 || p.t == 2 || p.t == 4 || p.t == 8 || p.t == 16), "unsupported q bits or t");
  Require(p.records % p.t == 0, "t must divide records");
}

inline uint32_t EvalCount(const Profile& p) { return p.t > 1 ? 6 : 0; }
inline unsigned NativeResponseBits(const Profile& p, unsigned bits) { return 64 - p.q_bits + bits; }
inline uint64_t NativeResponseLimit(const Profile& p, unsigned bits) { return uint64_t{1} << NativeResponseBits(p, bits); }

inline std::array<uint32_t, 14> Geometry(const Profile& p) {
  CheckProfile(p);
  return {p.records, kDegree, kShards, p.t, kPlaintext, 40, 19, 2, 19, 3, p.q_bits, kQ1Bits, kQ2Bits, kRecordBytes};
}

inline Bytes Frame(Kind kind, const Profile& profile, uint64_t sequence, const Bytes& payload) {
  CheckProfile(profile);
  Require(payload.size() <= kMaxFrameBytes - kHeaderBytes, "oversized payload");
  Bytes out;
  out.reserve(kHeaderBytes + payload.size());
  for (uint8_t c : std::array<uint8_t, 8>{'A', 'L', 'T', 'R', 'P', 'I', 'R', '1'}) out.push_back(c);
  Put(out, 1, 4);
  Put(out, static_cast<uint32_t>(kind), 4);
  Put(out, kPacked, 4);
  for (uint32_t field : Geometry(profile)) Put(out, field, 4);
  Put(out, sequence, 8);
  Put(out, payload.size(), 8);
  out.insert(out.end(), payload.begin(), payload.end());
  return out;
}

inline Reader Open(const Bytes& frame, Kind kind, const Profile& profile, uint64_t sequence) {
  CheckProfile(profile);
  Require(frame.size() >= kHeaderBytes && frame.size() <= kMaxFrameBytes, "invalid frame size");
  Reader in{frame};
  for (uint8_t c : std::array<uint8_t, 8>{'A', 'L', 'T', 'R', 'P', 'I', 'R', '1'}) Require(in.Get(1) == c, "wrong magic");
  Require(in.Get(4) == 1, "wrong version");
  Require(in.Get(4) == static_cast<uint32_t>(kind), "wrong message kind");
  Require(in.Get(4) == kPacked, "wrong encoding");
  for (uint32_t field : Geometry(profile)) Require(in.Get(4) == field, "geometry mismatch");
  Require(in.Get(8) == sequence, "wrong sequence");
  Require(in.Get(8) == frame.size() - kHeaderBytes, "payload length mismatch");
  return in;
}

inline void Values(Bytes& out, const std::vector<uint64_t>& values, unsigned width) {
  Require(width > 0 && width <= 56, "invalid width");
  uint64_t acc = 0;
  unsigned used = 0;
  for (uint64_t value : values) {
    Require(value <= Mask(width), "noncanonical outgoing coefficient");
    acc |= value << used;
    used += width;
    while (used >= 8) {
      out.push_back(uint8_t(acc));
      acc >>= 8;
      used -= 8;
    }
  }
  if (used) out.push_back(uint8_t(acc));
}

inline std::vector<uint64_t> Values(Reader& in, size_t count, unsigned width) {
  Require(count <= 2097152 && width > 0 && width <= 56, "coefficient shape exceeds limit");
  const size_t bytes = (count * width + 7) / 8;
  Require(in.pos <= in.data.size() && bytes <= in.data.size() - in.pos, "truncated coefficients");
  std::vector<uint64_t> out(count);
  uint64_t acc = 0;
  unsigned used = 0;
  for (auto& value : out) {
    while (used < width) {
      acc |= in.Get(1) << used;
      used += 8;
    }
    value = acc & Mask(width);
    acc >>= width;
    used -= width;
  }
  Require(acc == 0, "nonzero unused bits");
  return out;
}

inline size_t RequestBytes(const Profile& p) {
  return kHeaderBytes + 24 + (size_t(p.records / p.t) * p.q_bits + 7) / 8 + (EvalCount(p) + 4) * ((size_t(kDegree) * p.q_bits + 7) / 8);
}

inline Bytes EncodeRequest(const Request& request, const Profile& profile, uint64_t sequence) {
  CheckProfile(profile);
  const size_t count = profile.records / profile.t, eval = EvalCount(profile);
  Require(request.first_dimension_query.size() == count && request.second_dimension_query.size() == eval && request.packing_key.size() == 4,
          "request shape mismatch");
  Bytes payload;
  Put(payload, count, 8);
  Put(payload, eval, 8);
  Put(payload, 4, 8);
  Values(payload, request.first_dimension_query, profile.q_bits);
  for (const auto* family : {&request.second_dimension_query, &request.packing_key})
    for (const auto& poly : *family) {
      Require(poly.Len() == kDegree, "request polynomial length");
      Values(payload, poly.Coeffs(), profile.q_bits);
    }
  return Frame(Kind::Request, profile, sequence, payload);
}

inline Request DecodeRequest(const Bytes& frame, const Profile& profile, uint64_t sequence) {
  auto in = Open(frame, Kind::Request, profile, sequence);
  const size_t count = profile.records / profile.t, eval = EvalCount(profile);
  Require(in.Get(8) == count && in.Get(8) == eval && in.Get(8) == 4, "request counts");
  Request request;
  request.first_dimension_query = Values(in, count, profile.q_bits);
  for (size_t i = 0; i < eval; ++i) request.second_dimension_query.push_back(Take(Poly::Create(Values(in, kDegree, profile.q_bits))));
  for (size_t i = 0; i < 4; ++i) request.packing_key.push_back(Take(Poly::Create(Values(in, kDegree, profile.q_bits))));
  in.End();
  return request;
}

inline Bytes EncodeResponse(const Response& response, const Profile& profile, uint64_t sequence) {
  Require(response.ciphertexts.size() == kShards, "response shard count");
  Bytes payload;
  payload.reserve(8 + 2 * kShards * kDegree * 8);
  Put(payload, kShards, 8);
  for (const auto& ct : response.ciphertexts) {
    Require(ct.a.Len() == kDegree && ct.b.Len() == kDegree, "response polynomial length");
    for (const auto& component : {std::make_pair(&ct.a, kQ1Bits), std::make_pair(&ct.b, kQ2Bits)}) {
      auto values = component.first->Coeffs();
      for (auto& value : values) {
        Require(value <= NativeResponseLimit(profile, component.second), "response exceeds native range");
        value &= Mask(component.second);
      }
      Values(payload, values, component.second);
    }
  }
  return Frame(Kind::Response, profile, sequence, payload);
}

inline Response DecodeResponse(const Bytes& frame, const Profile& profile, uint64_t sequence) {
  auto in = Open(frame, Kind::Response, profile, sequence);
  Require(in.Get(8) == kShards, "response shard count");
  Response response;
  for (size_t i = 0; i < kShards; ++i) {
    auto a = Take(Poly::Create(Values(in, kDegree, kQ1Bits)));
    auto b = Take(Poly::Create(Values(in, kDegree, kQ2Bits)));
    response.ciphertexts.push_back({std::move(a), std::move(b)});
  }
  in.End();
  return response;
}
}  // namespace bench
#endif
