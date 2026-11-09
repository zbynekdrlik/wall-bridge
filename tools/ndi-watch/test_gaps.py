import unittest

from gaps import Tracker

FRAME = 1 / 60  # seconds
TICK = 10_000_000 // 60  # NDI 100 ns units per frame


def run(tracker, frames):
    """frames: list of (recv_s, send_ticks); returns the gaps found."""
    return [g for g in (tracker.feed(r, s) for r, s in frames) if g]


class TrackerTest(unittest.TestCase):
    def test_even_frames_have_no_gap(self):
        t = Tracker()
        frames = [(100 + i * FRAME, i * TICK) for i in range(600)]
        self.assertEqual(run(t, frames), [])
        self.assertEqual(t.summary.frames, 600)

    def test_sender_pause_is_a_sender_gap(self):
        t = Tracker()
        frames = [(100 + i * FRAME, i * TICK) for i in range(10)]
        # 200 ms with no frames on either clock.
        frames.append((100 + 9 * FRAME + 0.2, 9 * TICK + 2_000_000))
        gaps = run(t, frames)
        self.assertEqual(len(gaps), 1)
        self.assertEqual(gaps[0].side, "sender")
        self.assertAlmostEqual(gaps[0].recv_ms, 200, delta=1)
        self.assertAlmostEqual(gaps[0].send_ms, 200, delta=1)

    def test_late_arrival_with_even_send_is_transport(self):
        t = Tracker()
        frames = [(100 + i * FRAME, i * TICK) for i in range(10)]
        # Next frame was sent on time but arrived 150 ms late.
        frames.append((100 + 9 * FRAME + 0.15, 10 * TICK))
        gaps = run(t, frames)
        self.assertEqual(len(gaps), 1)
        self.assertEqual(gaps[0].side, "transport")

    def test_two_frame_jitter_is_not_a_gap(self):
        t = Tracker()
        frames = [(100.0, 0), (100 + 2 * FRAME, 2 * TICK)]
        self.assertEqual(run(t, frames), [])

    def test_rate_sets_threshold(self):
        t = Tracker()
        t.set_rate(25, 1)  # 40 ms frames: 80 ms is normal-ish, not a gap
        frames = [(100.0, 0), (100.08, 800_000)]
        self.assertEqual(run(t, frames), [])

    def test_reset_skips_the_reconnect_pause(self):
        t = Tracker()
        run(t, [(100.0, 0), (100 + FRAME, TICK)])
        t.reset()
        self.assertEqual(run(t, [(105.0, 50_000_000)]), [])

    def test_summary_rolls_each_minute(self):
        t = Tracker()
        frames = [(59.5 + i * FRAME, i * TICK) for i in range(60)]  # crosses 60 s
        run(t, frames)
        done = t.take_finished()
        self.assertEqual(len(done), 1)
        self.assertEqual(done[0].start, 0)
        self.assertEqual(done[0].frames + t.summary.frames, 60)
        self.assertEqual(t.take_finished(), [])

    def test_summary_counts_gaps_and_max(self):
        t = Tracker()
        frames = [(100 + i * FRAME, i * TICK) for i in range(5)]
        frames.append((100 + 4 * FRAME + 0.3, 4 * TICK + 3_000_000))
        run(t, frames)
        s = t.summary
        self.assertEqual(s.gaps, 1)
        self.assertEqual(s.sender_gaps, 1)
        self.assertAlmostEqual(s.max_recv_ms, 300, delta=1)


if __name__ == "__main__":
    unittest.main()
