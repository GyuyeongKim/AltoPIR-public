#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <fstream>
#include <iostream>
#include <memory>
#include <sstream>
#include <string>
#include <sys/syscall.h>
#include <unistd.h>
#include <vector>

#include "crypto/pir/client/pir_client.h"
#include "crypto/pir/server/preprocessing_server.h"
#include "fixture.h"
#include "pool.h"
#include "shell_encryption/prng/chacha_prng.h"
#include "wire.h"

namespace pir = private_membership::rlwe::v2;
using Client = pir::PirClient<uint64_t>;
using Prep = pir::PreprocessingServer<uint16_t, uint64_t, int32_t>;
using bench::Require;
using bench::Take;

struct Cell {
  unsigned batch, cores;
};

struct Options {
  bench::Profile profile;
  std::vector<int> cpus;
  std::vector<Cell> cells;
  unsigned trials = 5;
};

std::vector<int> ParseList(const std::string& s) {
  std::vector<int> out;
  std::stringstream in(s);
  std::string part;
  while (std::getline(in, part, ',')) {
    const auto dash = part.find('-');
    if (dash == std::string::npos) {
      out.push_back(std::stoi(part));
    } else {
      for (int c = std::stoi(part.substr(0, dash)); c <= std::stoi(part.substr(dash + 1)); ++c) out.push_back(c);
    }
  }
  return out;
}

Options ParseOptions(int argc, char** argv) {
  Options o;
  const char* usage = "usage: reinspire-bench --rows N --q-bits 52|54 --t T --cpus LIST --cells B:C[,...] [--trials T]";
  for (int i = 1; i + 1 < argc; i += 2) {
    const std::string key(argv[i]), value(argv[i + 1]);
    if (key == "--rows") o.profile.records = uint32_t(std::stoul(value));
    else if (key == "--q-bits") o.profile.q_bits = unsigned(std::stoul(value));
    else if (key == "--t") o.profile.t = unsigned(std::stoul(value));
    else if (key == "--cpus") o.cpus = ParseList(value);
    else if (key == "--trials") o.trials = unsigned(std::stoul(value));
    else if (key == "--cells") {
      std::stringstream in(value);
      std::string cell;
      while (std::getline(in, cell, ',')) {
        const auto colon = cell.find(':');
        Require(colon != std::string::npos, usage);
        o.cells.push_back({unsigned(std::stoul(cell.substr(0, colon))), unsigned(std::stoul(cell.substr(colon + 1)))});
      }
    } else throw std::runtime_error(usage);
  }
  Require(argc % 2 == 1 && !o.cpus.empty() && !o.cells.empty(), usage);
  bench::CheckProfile(o.profile);
  for (const auto& c : o.cells)
    Require(c.batch > 0 && c.cores > 0 && c.cores <= o.cpus.size() && c.batch % std::min(c.batch, c.cores) == 0,
            "every cell needs C <= number of CPUs and a batch that splits evenly over min(B, C) workers");
  return o;
}

int CpuNode(int cpu) {
  for (int n = 0; n < 1024; ++n) {
    std::ifstream f("/sys/devices/system/cpu/cpu" + std::to_string(cpu) + "/node" + std::to_string(n) + "/cpulist");
    if (f.good()) return n;
  }
  return -1;
}

void MoveMemory(int cpu) {
  const int target = CpuNode(cpu);
  std::ifstream f("/sys/devices/system/node/online");
  std::string online;
  if (target < 0 || !std::getline(f, online)) return;
  const auto nodes = ParseList(online);
  const int max = std::max(*std::max_element(nodes.begin(), nodes.end()), target) + 1;
  std::vector<unsigned long> from((max + 63) / 64), to((max + 63) / 64);
  for (int n : nodes)
    if (n != target) from[n / 64] |= 1UL << (n % 64);
  to[target / 64] |= 1UL << (target % 64);
  if (std::any_of(from.begin(), from.end(), [](unsigned long w) { return w != 0; }))
    syscall(SYS_migrate_pages, 0, max + 1, from.data(), to.data());
}

double Median(std::vector<double> v) {
  std::sort(v.begin(), v.end());
  const size_t n = v.size();
  return n % 2 ? v[n / 2] : (v[n / 2 - 1] + v[n / 2]) / 2;
}

int Run(const Options& o) {
  bench::Pin(getpid(), o.cpus);
  const auto& profile = o.profile;
  const auto rlwe = Take(pir::RlweParams<uint64_t>::Create(2048, uint64_t{1} << profile.q_bits, uint64_t{1} << 29, uint64_t{1} << 18, 40));
  const auto params = Take(pir::PirParams<uint64_t>::Create(rlwe, 65535, pir::GadgetParams{19, 2}, pir::GadgetParams{19, 3}, profile.records, profile.t, 8));
  const auto crs = Take(::rlwe::ChaChaPrng::GenerateSeed());
  const auto begin = bench::NowNs();
  std::vector<uint16_t> db(size_t(profile.records) * bench::kWords);
  for (uint32_t i = 0; i < profile.records; ++i) {
    const auto row = bench::Record(i);
    std::copy(row.begin(), row.end(), db.begin() + size_t(i) * bench::kWords);
  }
  auto prep = Take(Prep::Create(&params, crs));
  auto preprocessed = Take(prep->Preprocess(db));
  auto server = Take(bench::Server::Create(params, std::move(preprocessed)));
  std::vector<uint16_t>().swap(db);
  prep.reset();
  const double setup_s = double(bench::NowNs() - begin) / 1e9;
  MoveMemory(o.cpus[0]);
  bench::Pool pool(*server, profile, o.cpus);
  std::printf("setup_s %.1f request_bytes %zu response_bytes %zu\n", setup_s, bench::RequestBytes(profile),
              bench::kHeaderBytes + 8 + bench::kShards * ((bench::kDegree * bench::kQ1Bits + 7) / 8 + (bench::kDegree * bench::kQ2Bits + 7) / 8));
  std::printf("%5s %5s %6s %12s %10s\n", "batch", "cores", "plan", "median_ms", "qps");
  std::fflush(stdout);
  uint64_t sequence = 0;
  for (const auto& cell : o.cells) {
    std::vector<double> times;
    for (unsigned trial = 0; trial <= o.trials; ++trial) {
      const auto indices = bench::Indices(trial, cell.batch, profile.records);
      std::vector<std::unique_ptr<Client>> clients;
      std::vector<bench::Envelope> inputs;
      for (unsigned i = 0; i < cell.batch; ++i) {
        const auto seed = Take(::rlwe::ChaChaPrng::GenerateSeed());
        auto client = Take(Client::Create(params, Take(::rlwe::ChaChaPrng::Create(crs)), Take(::rlwe::ChaChaPrng::Create(seed))));
        const auto request = Take(client->CreateRequest(indices[i]));
        ++sequence;
        inputs.push_back({sequence, bench::EncodeRequest(request, profile, sequence)});
        clients.push_back(std::move(client));
      }
      const auto answer = pool.Run(inputs, cell.cores);
      for (size_t i = 0; i < inputs.size(); ++i) {
        const auto response = bench::DecodeResponse(answer.replies[i].wire, profile, inputs[i].sequence);
        const auto recovered = Take(clients[i]->ProcessResponse(response));
        const auto expected = bench::Record(indices[i]);
        Require(std::equal(recovered.begin(), recovered.end(), expected.begin(), expected.end()), "wrong record");
      }
      if (trial > 0) times.push_back(double(answer.end_ns - answer.start_ns) / 1e6);
    }
    const double ms = Median(times);
    std::printf("%5u %5u %6s %12.2f %10.3f\n", cell.batch, cell.cores, "-", ms, cell.batch * 1e3 / ms);
    std::fflush(stdout);
  }
  pool.Stop();
  return 0;
}

int main(int argc, char** argv) {
  try {
    return Run(ParseOptions(argc, argv));
  } catch (const std::exception& e) {
    std::cerr << "reinspire-bench: " << e.what() << '\n';
    return 1;
  }
}
