"""Frame gap tracking for one NDI source (pure, no NDI import).

Two clocks per frame:
- recv: when the frame reached us (our wall clock, seconds);
- send: the sender's NDI timestamp (100 ns units), stamped on songs-snv.

A gap in both clocks means the sender produced no frames (songs-snv side). A gap on
recv while send stays even means frames were produced but arrived late (network or
transport). Each summary covers one minute so a service can be read at a glance.
"""

from dataclasses import dataclass, field

# A gap is an interval of more than this many expected frame intervals.
GAP_FACTOR = 2.5
# NDIlib_recv_timestamp_undefined: the sender did not stamp the frame.
TIMESTAMP_UNDEFINED = 0x7FFFFFFFFFFFFFFF


@dataclass
class Gap:
    at: float  # recv time of the frame that ended the gap
    recv_ms: float
    send_ms: float | None  # None when either frame had no sender timestamp
    expected_ms: float

    @property
    def side(self) -> str:
        """Where the gap came from, judged by the two clocks."""
        if self.send_ms is None:
            return "unknown"
        limit = self.expected_ms * GAP_FACTOR
        if self.send_ms > limit:
            return "sender"
        return "transport"


@dataclass
class Summary:
    start: float
    frames: int = 0
    gaps: int = 0
    max_recv_ms: float = 0.0
    max_send_ms: float = 0.0
    sender_gaps: int = 0


@dataclass
class Tracker:
    """Feed every received video frame; returns a Gap when one ended."""

    expected_ms: float = 1000 / 60
    last_recv: float | None = None
    last_send: int | None = None
    summary: Summary | None = None
    finished: list = field(default_factory=list)

    def set_rate(self, n: int, d: int) -> None:
        if n > 0 and d > 0:
            self.expected_ms = 1000 * d / n

    def reset(self) -> None:
        """Forget the last frame, e.g. after a reconnect (that pause is logged apart)."""
        self.last_recv = None
        self.last_send = None

    def feed(self, recv: float, send: int) -> Gap | None:
        self._roll(recv)
        s = self.summary
        s.frames += 1
        gap = None
        if self.last_recv is not None:
            recv_ms = (recv - self.last_recv) * 1000
            stamped = TIMESTAMP_UNDEFINED not in (send, self.last_send)
            send_ms = (send - self.last_send) / 1e4 if stamped else None
            s.max_recv_ms = max(s.max_recv_ms, recv_ms)
            if send_ms is not None:
                s.max_send_ms = max(s.max_send_ms, send_ms)
            limit = self.expected_ms * GAP_FACTOR
            if recv_ms > limit or (send_ms or 0) > limit:
                gap = Gap(recv, recv_ms, send_ms, self.expected_ms)
                s.gaps += 1
                if gap.side == "sender":
                    s.sender_gaps += 1
        self.last_recv = recv
        self.last_send = send
        return gap

    def tick(self, now: float) -> None:
        """Close the minute even when no frame comes (during a stall)."""
        if self.summary is not None:
            self._roll(now)

    def _roll(self, now: float) -> None:
        minute = now - now % 60
        if self.summary is None:
            self.summary = Summary(minute)
        elif minute > self.summary.start:
            self.finished.append(self.summary)
            self.summary = Summary(minute)

    def take_finished(self) -> list:
        out, self.finished = self.finished, []
        return out
