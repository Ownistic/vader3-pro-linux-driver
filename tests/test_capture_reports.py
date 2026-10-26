import io
import json
import select
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools import capture_reports


class CaptureReportsTests(unittest.TestCase):
    def test_vid_pid_accept_decimal_and_hex(self):
        args = capture_reports.parser().parse_args(["--vid", "1", "--pid", "0x41"])
        self.assertEqual((args.vid, args.pid), (1, 0x41))
        args = capture_reports.parser().parse_args(["--vid", "0xd7d7", "--pid", "65"])
        self.assertEqual((args.vid, args.pid), (0xD7D7, 65))

    def test_invalid_arguments_exit_nonzero(self):
        for options in (("--vid", "0x10000"), ("--vid", "x"), ("--max-seconds", "-1")):
            with self.subTest(options=options), mock.patch("sys.stderr", new_callable=io.StringIO):
                with self.assertRaises(SystemExit) as raised:
                    capture_reports.parser().parse_args(["--vid", "1", "--pid", "2", *options])
                self.assertNotEqual(raised.exception.code, 0)

    def test_synthetic_reports_round_trip_and_stop_gracefully(self):
        class Poller:
            def register(self, _fd, _events):
                pass

            def poll(self, _timeout):
                return [(7, select.POLLIN)]

        stop = [False]
        packets = iter((b"abc", b"def"))

        def read(_fd, _size):
            packet = next(packets)
            if packet == b"def":
                stop[0] = True
            return packet

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.jsonl"
            with path.open("w") as output, mock.patch.object(capture_reports, "descriptor_digest", return_value=None):
                capture_reports.capture_fd(7, output, "/dev/hidraw0", 1, 2, None, 30, 10,
                                           poller=Poller(), should_stop=lambda: stop[0], reader=read)
            records = [json.loads(line) for line in path.read_text().splitlines()]
        self.assertEqual([record["kind"] for record in records],
                         ["capture-start", "input", "input", "capture-end"])
        self.assertEqual([record["hex"] for record in records[1:3]], ["616263", "646566"])
        self.assertEqual(records[-1], {"kind": "capture-end", "reports": 2,
                                       "lengths": {"3": 2}, "reason": "requested-stop"})


if __name__ == "__main__":
    unittest.main()
