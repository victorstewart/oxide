#!/usr/bin/env python3
"""Reduce exported Instruments CPU samples inside the marked warm interval."""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET


class References:
    def __init__(self, root):
        self.values = {element.attrib["id"]: element for element in root.iter() if "id" in element.attrib}

    def get(self, element):
        return self.values[element.attrib["ref"]] if "ref" in element.attrib else element

    def value(self, element):
        return self.get(element).text


def interval(path):
    root = ET.parse(path).getroot()
    refs = References(root)
    points = set()
    for row in root.findall(".//row"):
        if len(row) > 6 and refs.value(row[6]) == "OxideOffscreenProfile":
            points.add((refs.value(row[3]), int(refs.value(row[0]))))
    begins = [time for event, time in points if event == "Begin"]
    ends = [time for event, time in points if event == "End"]
    if len(begins) != 1 or len(ends) != 1 or ends[0] <= begins[0]:
        raise ValueError("one complete warm profile interval is required")
    return begins[0], ends[0]


def reduce(samples, signposts):
    begin, end = interval(signposts)
    root = ET.parse(samples).getroot()
    refs = References(root)
    names = sorted({element.get("name") for element in root.iter("frame") if element.get("name")})
    demangled = subprocess.run(["xcrun", "llvm-cxxfilt"], input="\n".join(names) + "\n", text=True,
                               check=True, stdout=subprocess.PIPE).stdout.splitlines()
    if len(names) != len(demangled):
        raise ValueError("demangler changed symbol line population")
    symbols = dict(zip(names, demangled))
    hot_inclusive, hot_self, hot_owner, startup = Counter(), Counter(), Counter(), Counter()
    total = 0
    sample_count = 0
    unknown = 0
    thread_weights = Counter()
    for row in root.findall(".//row"):
        if len(row) != 7:
            continue
        time = int(refs.value(row[0]))
        weight = int(refs.value(row[5]))
        stack = refs.get(row[6])
        frames = [refs.get(frame) for frame in stack.findall("frame")]
        names = [symbols.get(frame.get("name"), frame.get("name", "unknown")) for frame in frames]
        if not names:
            continue
        if time < begin:
            for name in set(names):
                if "oxide_" in name:
                    startup[name] += weight
            continue
        if time >= end:
            continue
        total += weight
        sample_count += 1
        thread_weights[refs.get(row[1]).get("fmt", "unknown")] += weight
        hot_self[names[0]] += weight
        if names[0].startswith("0x"):
            unknown += weight
        for name in set(names):
            hot_inclusive[name] += weight
        owner = next((name for name in names if "oxide_" in name), "system/driver threads without an Oxide caller")
        hot_owner[owner] += weight

    def ranked(counter, count=40):
        return [{"symbol": name, "sampled_ms": weight / 1e6,
                 "percent_of_warm_cpu_samples": 100 * weight / total if total else None}
                for name, weight in counter.most_common(count)]

    return {"schema": 1, "scope": "statistical running-thread CPU samples inside warm signpost; inclusive rows overlap",
            "begin_ns": begin, "end_ns": end, "samples": sample_count, "sampled_cpu_ms": total / 1e6,
            "unknown_leaf_percent": 100 * unknown / total if total else None,
            "self": ranked(hot_self), "inclusive_oxide": ranked(Counter({k: v for k, v in hot_inclusive.items() if "oxide_" in k})),
            "nearest_oxide_owner": ranked(hot_owner),
            "startup_inclusive_oxide_ms": [{"symbol": name, "sampled_ms": weight / 1e6} for name, weight in startup.most_common(25)],
            "threads_ms": {name: weight / 1e6 for name, weight in thread_weights.items()}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", required=True, type=Path)
    parser.add_argument("--signposts", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = reduce(args.samples, args.signposts)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ("samples", "sampled_cpu_ms", "unknown_leaf_percent")}))


if __name__ == "__main__":
    main()
