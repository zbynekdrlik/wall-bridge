# Latency measurement, step 2: python lat_pair.py <events.json> <arrivals.txt>
# arrivals.txt: one line per new Spout picture, epoch microseconds first (the MVP's --arrivals log).
# Needs a static scene with text cuts: on moving video every picture is an arrival, so the pairing
# only returns the time since the last picture and means nothing.
import json, sys, statistics as st

# Pair each grabber change event with the Spout arrival of that picture: the last arrival
# within 300 ms before it. Prints the arrival -> captured delay distribution in ms.
ev = [e["t_ms"] for e in json.load(open(sys.argv[1]))["events"]]
arr = []
for line in open(sys.argv[2]):
    p = line.split()
    if p:
        arr.append(int(p[0]) / 1000.0)
d = []
for e in ev:
    before = [e - a for a in arr if 0 < e - a < 300]
    if before:
        d.append(min(before))
d.sort()
print(json.dumps({"events": len(ev), "arrivals": len(arr), "paired": len(d),
                  "median_ms": round(st.median(d), 1), "mean_ms": round(st.mean(d), 1),
                  "min": round(d[0], 1), "max": round(d[-1], 1), "all": [round(x, 1) for x in d]}))
