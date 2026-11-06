#pragma once

// In-memory WaveRT link for one adapter. Both pins use 48 kHz stereo PCM16.
// All operations are nonpageable and valid from the position timer at DISPATCH_LEVEL.
class VoiceBridge
{
    static constexpr ULONG FrameBytes = 4;
    static constexpr ULONG Capacity = 48000 * FrameBytes * 60 / 1000;
    static constexpr ULONG Startup = 48000 * FrameBytes * 10 / 1000;

    KSPIN_LOCK lock_;
    const void* render_;
    const void* capture_;
    bool protected_;
    bool primed_;
    ULONG read_;
    ULONG size_;
    UCHAR data_[Capacity];

    void Reset()
    {
        read_ = size_ = 0;
        primed_ = false;
        RtlSecureZeroMemory(data_, sizeof(data_));
    }

public:
    void Initialize()
    {
        KeInitializeSpinLock(&lock_);
        render_ = capture_ = nullptr;
        protected_ = false;
        Reset();
    }

    void SetRunning(const void* stream, bool capture, bool running, bool isProtected)
    {
        KIRQL previous;
        KeAcquireSpinLock(&lock_, &previous);
        const void*& slot = capture ? capture_ : render_;
        if (running && slot != stream)
        {
            slot = stream;
            if (!capture) protected_ = isProtected;
            Reset();
        }
        else if (!running && slot == stream)
        {
            slot = nullptr;
            Reset();
        }
        KeReleaseSpinLock(&lock_, previous);
    }

    void Protect(const void* stream, bool isProtected)
    {
        KIRQL previous;
        KeAcquireSpinLock(&lock_, &previous);
        if (render_ == stream && protected_ != isProtected)
        {
            protected_ = isProtected;
            Reset();
        }
        KeReleaseSpinLock(&lock_, previous);
    }

    void Push(const void* stream, const UCHAR* source, ULONG bytes)
    {
        bytes -= bytes % FrameBytes;
        if (bytes > Capacity)
        {
            source += bytes - Capacity;
            bytes = Capacity;
        }
        KIRQL previous;
        KeAcquireSpinLock(&lock_, &previous);
        if (render_ == stream && capture_ && !protected_)
        {
            ULONG free = Capacity - size_;
            if (bytes > free)
            {
                ULONG discarded = bytes - free;
                read_ = (read_ + discarded) % Capacity;
                size_ -= discarded;
            }
            ULONG write = (read_ + size_) % Capacity;
            ULONG first = min(bytes, Capacity - write);
            RtlCopyMemory(data_ + write, source, first);
            RtlCopyMemory(data_, source + first, bytes - first);
            size_ += bytes;
        }
        KeReleaseSpinLock(&lock_, previous);
    }

    void Pull(const void* stream, UCHAR* target, ULONG bytes)
    {
        KIRQL previous;
        KeAcquireSpinLock(&lock_, &previous);
        ULONG available = 0;
        if (capture_ == stream && render_ && !protected_ && (primed_ || size_ >= Startup))
        {
            available = min(bytes - bytes % FrameBytes, size_);
            ULONG first = min(available, Capacity - read_);
            RtlCopyMemory(target, data_ + read_, first);
            RtlCopyMemory(target + first, data_, available - first);
            read_ = (read_ + available) % Capacity;
            size_ -= available;
            primed_ = available == bytes;
        }
        KeReleaseSpinLock(&lock_, previous);
        RtlZeroMemory(target + available, bytes - available);
    }
};
