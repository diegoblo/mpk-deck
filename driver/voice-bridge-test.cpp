#include <algorithm>
#include <array>
#include <cassert>
#include <cstdint>
#include <cstring>

// Test the same FIFO implementation in user mode. The shims do not test IRQL or locking.
using ULONG = std::uint32_t;
using UCHAR = unsigned char;
using KIRQL = int;
using KSPIN_LOCK = int;
using std::min;
void KeInitializeSpinLock(KSPIN_LOCK* lock) { *lock = 0; }
void KeAcquireSpinLock(KSPIN_LOCK*, KIRQL* previous) { *previous = 0; }
void KeReleaseSpinLock(KSPIN_LOCK*, KIRQL) {}
void RtlSecureZeroMemory(void* buffer, std::size_t bytes) { std::memset(buffer, 0, bytes); }
void RtlZeroMemory(void* buffer, std::size_t bytes) { std::memset(buffer, 0, bytes); }
void RtlCopyMemory(void* target, const void* source, std::size_t bytes) { std::memcpy(target, source, bytes); }

#include "voice-bridge.h"

int main()
{
    VoiceBridge bridge;
    bridge.Initialize();
    int render = 0;
    int capture = 0;
    bridge.SetRunning(&render, false, true, false);
    bridge.SetRunning(&capture, true, true, false);

    std::array<UCHAR, 12000> input{};
    for (std::size_t i = 0; i < input.size(); ++i) input[i] = static_cast<UCHAR>(i % 251);
    std::array<UCHAR, 11520> output{};
    bridge.Pull(&capture, output.data(), static_cast<ULONG>(output.size()));
    assert(std::all_of(output.begin(), output.end(), [](UCHAR value) { return value == 0; }));

    // Keeping only the newest complete frames makes the wrap and overflow deterministic.
    bridge.Push(&render, input.data(), 6000);
    bridge.Push(&render, input.data() + 6000, 6000);
    bridge.Pull(&capture, output.data(), static_cast<ULONG>(output.size()));
    assert(std::equal(output.begin(), output.end(), input.end() - output.size()));

    bridge.Push(&render, input.data(), 6000);
    bridge.Protect(&render, true);
    bridge.Pull(&capture, output.data(), static_cast<ULONG>(output.size()));
    assert(std::all_of(output.begin(), output.end(), [](UCHAR value) { return value == 0; }));
    bridge.Push(&render, input.data(), 6000);
    bridge.Protect(&render, false);
    bridge.Pull(&capture, output.data(), static_cast<ULONG>(output.size()));
    assert(std::all_of(output.begin(), output.end(), [](UCHAR value) { return value == 0; }));

    bridge.Push(&render, input.data(), 6000);
    bridge.SetRunning(&render, false, false, false);
    bridge.SetRunning(&render, false, true, false);
    bridge.Pull(&capture, output.data(), static_cast<ULONG>(output.size()));
    assert(std::all_of(output.begin(), output.end(), [](UCHAR value) { return value == 0; }));
}
