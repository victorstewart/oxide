from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import reduce_macos_profile as reducer


class ProfileTests(unittest.TestCase):
    def test_reference_rows_and_duplicate_signposts_keep_only_warm_samples(self):
        signs = '''<root><row><time id="1">10</time><x/><x/><event id="2">Begin</event><x/><x/><name id="3">OxideOffscreenProfile</name></row>
        <row><time ref="1"/><x/><x/><event ref="2"/><x/><x/><name ref="3"/></row>
        <row><time>20</time><x/><x/><event>End</event><x/><x/><name ref="3"/></row></root>'''
        samples = '''<root><row><time>9</time><thread id="1" fmt="main"/><p/><c/><s/><weight id="2">1000000</weight><stack id="3"><frame id="4" name="oxide_text::bake"/><frame name="oxide_runtime::draw"/></stack></row>
        <row><time>10</time><thread ref="1"/><p/><c/><s/><weight ref="2"/><stack ref="3"/></row>
        <row><time>19</time><thread ref="1"/><p/><c/><s/><weight>2000000</weight><stack><frame ref="4"/><frame ref="4"/><frame name="oxide_runtime::draw"/></stack></row>
        <row><time>20</time><thread ref="1"/><p/><c/><s/><weight ref="2"/><stack ref="3"/></row></root>'''
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "signs.xml").write_text(signs)
            (root / "samples.xml").write_text(samples)
            def demangle(*args, **kwargs):
                return type("Result", (), {"stdout": kwargs["input"]})()
            with patch.object(reducer.subprocess, "run", side_effect=demangle):
                result = reducer.reduce(root / "samples.xml", root / "signs.xml")
        self.assertEqual(result["samples"], 2)
        self.assertEqual(result["sampled_cpu_ms"], 3)
        self.assertEqual(result["nearest_oxide_owner"][0]["sampled_ms"], 3)
        self.assertEqual(result["inclusive_oxide"][0]["sampled_ms"], 3)
        self.assertEqual(result["startup_inclusive_oxide_ms"][0]["sampled_ms"], 1)

    def test_missing_signpost_end_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "signs.xml"
            path.write_text("<root/>")
            with self.assertRaises(ValueError):
                reducer.interval(path)


if __name__ == "__main__":
    unittest.main()
