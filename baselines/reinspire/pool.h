#ifndef REINSPIRE_BENCH_POOL_H_
#define REINSPIRE_BENCH_POOL_H_

#include <algorithm>
#include <cerrno>
#include <chrono>
#include <csignal>
#include <cstdio>
#include <iostream>
#include <sched.h>
#include <sys/prctl.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <vector>

#include "crypto/pir/server/pir_server.h"
#include "wire.h"

namespace bench {
using Server = pir::PirServer<uint16_t, uint64_t, int32_t>;

inline uint64_t NowNs() {
  return uint64_t(std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now().time_since_epoch()).count());
}

inline void WriteAll(int fd, const void* ptr, size_t bytes) {
  const char* p = static_cast<const char*>(ptr);
  while (bytes) {
    const auto n = write(fd, p, bytes);
    if (n < 0 && errno == EINTR) continue;
    Require(n > 0, "worker pipe write failed");
    p += n;
    bytes -= size_t(n);
  }
}

inline void ReadAll(int fd, void* ptr, size_t bytes) {
  char* p = static_cast<char*>(ptr);
  while (bytes) {
    const auto n = read(fd, p, bytes);
    if (n < 0 && errno == EINTR) continue;
    Require(n > 0, "worker pipe read failed");
    p += n;
    bytes -= size_t(n);
  }
}

inline void SendU64(int fd, uint64_t x) {
  unsigned char b[8];
  for (unsigned i = 0; i < 8; ++i) b[i] = uint8_t(x >> (8 * i));
  WriteAll(fd, b, 8);
}

inline uint64_t ReceiveU64(int fd) {
  unsigned char b[8];
  ReadAll(fd, b, 8);
  uint64_t x = 0;
  for (unsigned i = 0; i < 8; ++i) x |= uint64_t(b[i]) << (8 * i);
  return x;
}

inline void SendBytes(int fd, const Bytes& b) {
  SendU64(fd, b.size());
  WriteAll(fd, b.data(), b.size());
}

inline Bytes ReceiveBytes(int fd) {
  const uint64_t n = ReceiveU64(fd);
  Require(n <= kMaxFrameBytes, "frame too large");
  Bytes b(size_t(n), 0);
  ReadAll(fd, b.data(), b.size());
  return b;
}

inline void Pin(pid_t pid, const std::vector<int>& cpus) {
  cpu_set_t set;
  CPU_ZERO(&set);
  for (int c : cpus) CPU_SET(c, &set);
  Require(sched_setaffinity(pid, sizeof(set), &set) == 0, "cannot set CPU affinity");
}

struct Envelope {
  uint64_t sequence;
  Bytes wire;
};

struct Reply {
  Bytes wire;
  uint64_t begin_ns = 0, end_ns = 0;
};

inline Reply Serve(const Server& server, const Envelope& input, const Profile& profile) {
  Reply out;
  out.begin_ns = NowNs();
  {
    auto request = DecodeRequest(input.wire, profile, input.sequence);
    auto response = Take(server.ProcessResponse(request));
    out.wire = EncodeResponse(response, profile, input.sequence);
  }
  out.end_ns = NowNs();
  return out;
}

class Pool {
 public:
  Pool(const Server& server, const Profile& profile, const std::vector<int>& cpus) : cpus_(cpus) {
    const pid_t owner = getpid();
    std::cout.flush();
    fflush(nullptr);
    for (size_t i = 0; i < cpus.size(); ++i) {
      int down[2], up[2];
      Require(pipe(down) == 0 && pipe(up) == 0, "pipe failed");
      const pid_t pid = fork();
      Require(pid >= 0, "fork failed");
      if (pid == 0) {
        close(down[1]);
        close(up[0]);
        for (const auto& old : children_) {
          close(old.to);
          close(old.from);
        }
        if (prctl(PR_SET_PDEATHSIG, SIGKILL) != 0 || getppid() != owner) _exit(110);
        if (dup2(STDERR_FILENO, STDOUT_FILENO) < 0) _exit(111);
        try {
          Worker(server, profile, down[0], up[1]);
          _exit(0);
        } catch (const std::exception& e) {
          std::cerr << "worker failed: " << e.what() << '\n';
          _exit(112);
        }
      }
      close(down[0]);
      close(up[1]);
      children_.push_back({pid, down[1], up[0]});
    }
  }
  Pool(const Pool&) = delete;
  Pool& operator=(const Pool&) = delete;
  ~Pool() {
    for (const auto& c : children_) kill(c.pid, SIGKILL);
    for (const auto& c : children_) {
      int s;
      while (waitpid(c.pid, &s, 0) < 0 && errno == EINTR) {
      }
    }
  }

  struct Batch {
    std::vector<Reply> replies;
    uint64_t start_ns = 0, end_ns = 0;
  };

  Batch Run(const std::vector<Envelope>& inputs, unsigned cores) {
    Pin(getpid(), std::vector<int>(cpus_.begin(), cpus_.begin() + cores));
    for (unsigned w = 0; w < children_.size(); ++w) Pin(children_[w].pid, {cpus_[cores == 1 ? 0 : w % cores]});
    const unsigned workers = std::min<unsigned>(unsigned(inputs.size()), cores);
    const size_t per = inputs.size() / workers;
    Require(per * workers == inputs.size(), "batch must split evenly over the workers");
    for (unsigned w = 0; w < workers; ++w) {
      auto& child = children_[w];
      SendU64(child.to, 1);
      SendU64(child.to, per);
      for (size_t i = w * per; i < (w + 1) * per; ++i) {
        SendU64(child.to, inputs[i].sequence);
        SendBytes(child.to, inputs[i].wire);
      }
    }
    for (unsigned w = 0; w < workers; ++w) Require(ReceiveU64(children_[w].from) == 0x5245414459ULL, "worker not ready");
    Batch out;
    out.replies.resize(inputs.size());
    out.start_ns = NowNs();
    for (unsigned w = 0; w < workers; ++w) SendU64(children_[w].to, 2);
    for (unsigned w = 0; w < workers; ++w) {
      auto& child = children_[w];
      Require(ReceiveU64(child.from) == 0x444f4e45ULL, "worker did not finish");
      for (size_t i = w * per; i < (w + 1) * per; ++i) {
        auto& r = out.replies[i];
        r.begin_ns = ReceiveU64(child.from);
        r.end_ns = ReceiveU64(child.from);
        out.end_ns = std::max(out.end_ns, r.end_ns);
      }
    }
    for (unsigned w = 0; w < workers; ++w) SendU64(children_[w].to, 3);
    for (unsigned w = 0; w < workers; ++w)
      for (size_t i = w * per; i < (w + 1) * per; ++i) out.replies[i].wire = ReceiveBytes(children_[w].from);
    return out;
  }

  void Stop() {
    for (auto& child : children_) SendU64(child.to, 0);
    for (auto& child : children_) {
      int status = 0;
      while (waitpid(child.pid, &status, 0) < 0 && errno == EINTR) {
      }
      close(child.to);
      close(child.from);
    }
    children_.clear();
  }

 private:
  struct Child {
    pid_t pid;
    int to, from;
  };
  std::vector<int> cpus_;
  std::vector<Child> children_;

  static void Worker(const Server& server, const Profile& profile, int down, int up) {
    for (;;) {
      const auto opcode = ReceiveU64(down);
      if (opcode == 0) return;
      Require(opcode == 1, "unexpected opcode");
      const auto count = ReceiveU64(down);
      std::vector<Envelope> inputs;
      inputs.reserve(size_t(count));
      for (size_t i = 0; i < count; ++i) {
        const auto seq = ReceiveU64(down);
        auto wire = ReceiveBytes(down);
        inputs.push_back({seq, std::move(wire)});
      }
      std::vector<Reply> replies;
      replies.reserve(size_t(count));
      SendU64(up, 0x5245414459ULL);
      Require(ReceiveU64(down) == 2, "unexpected opcode");
      for (const auto& input : inputs) replies.push_back(Serve(server, input, profile));
      SendU64(up, 0x444f4e45ULL);
      for (const auto& r : replies) {
        SendU64(up, r.begin_ns);
        SendU64(up, r.end_ns);
      }
      Require(ReceiveU64(down) == 3, "unexpected opcode");
      for (const auto& r : replies) SendBytes(up, r.wire);
      std::vector<Envelope>().swap(inputs);
      std::vector<Reply>().swap(replies);
    }
  }
};
}  // namespace bench
#endif
