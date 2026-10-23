import csv
import glob
import json
import os
import re
import statistics
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.ticker
from matplotlib.lines import Line2D
from matplotlib.patches import Patch

OUT = sys.argv[1] if len(sys.argv) > 1 else "results"
CORES = int(sys.argv[2]) if len(sys.argv) > 2 else 16
RED, BLUE, GREEN, GOLD, VIOLET = "#D85A4D", "#3D6FA0", "#4A9D7A", "#D9A65A", "#8E72B5"
SCHEMES = [("ypir", "YPIR+SP", VIOLET, "v"), ("inspire", "InsPIRe", GOLD, "s"), ("valar", "Valar", GREEN, "D"),
           ("reinspire", "ReinsPIRe", BLUE, "o"), ("altopir", "AltoPIR", RED, "^")]
summary = []


def numbers(path):
    for line in open(path):
        f = line.split()
        if f and f[0].isdigit():
            yield f


def combine(samples):
    ms = statistics.median(samples)
    return ms, min(samples), max(samples)


def altopir_rows(name):
    path = os.path.join(OUT, name)
    if not os.path.exists(path):
        return {}
    return {int(f[0]): float(f[2]) for f in numbers(path) if len(f) == 4}


def baseline_cells():
    cells = {}
    for path in glob.glob(os.path.join(OUT, "*_p*.txt")):
        m = re.match(r"(reinspire|valar|inspire)_(\d+)gb(?:_1core)?_p\d+\.txt$", os.path.basename(path))
        if not m:
            continue
        for f in numbers(path):
            if len(f) == 5:
                key = (m.group(1), int(m.group(2)), int(f[0]), int(f[1]), f[2])
                cells.setdefault(key, []).append(float(f[3]))
    for path in glob.glob(os.path.join(OUT, "ypir_*gb_p*.json")):
        g = int(re.search(r"ypir_(\d+)gb", path).group(1))
        times = json.load(open(path))["online"]["allServerTimesMs"]
        cells.setdefault(("ypir", g, 1, 1, "-"), []).append(statistics.median(times))
    return {k: combine(v) for k, v in cells.items()}


def points():
    cells = baseline_cells()
    out = {"c16": {}, "c1": {}}
    for (s, g, b, c, plan), (ms, lo, hi) in cells.items():
        q = (b * 1e3 / ms, b * 1e3 / hi, b * 1e3 / lo)
        summary.append([s, g, c, b, plan, round(ms, 2), round(q[0], 3)])
        if c == CORES and q[0] > out["c16"].get((s, g), (0,))[0]:
            out["c16"][(s, g)] = q
        if c == 1 and b == 1:
            out["c1"][(s, g)] = q
    for path in glob.glob(os.path.join(OUT, "altopir_*gb*.txt")):
        m = re.match(r"altopir_(\d+)gb(_1core)?\.txt$", os.path.basename(path))
        if not m:
            continue
        g, panel = int(m.group(1)), "c1" if m.group(2) else "c16"
        rows = altopir_rows(os.path.basename(path))
        if not rows:
            continue
        for b, ms in rows.items():
            summary.append(["altopir", g, 1 if m.group(2) else CORES, b, "-", round(ms, 2), round(b * 1e3 / ms, 3)])
        best = max(rows, key=lambda b: b / rows[b])
        out[panel][("altopir", g)] = (best * 1e3 / rows[best],) * 3
        if 1 in rows:
            out[panel][("altopir_b1", g)] = (1e3 / rows[1],) * 3
    return out


def style(ax, xlabel, ylabel):
    ax.set_xlabel(xlabel)
    ax.set_ylabel(ylabel)
    ax.grid(True, which="major", alpha=0.3)
    ax.set_axisbelow(True)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)


def save(fig, name):
    for ext in ("png", "pdf"):
        fig.savefig(os.path.join(OUT, f"{name}.{ext}"), dpi=200, bbox_inches="tight")
    plt.close(fig)


def comparison(p):
    sizes = [g for g in (1, 8) if ("altopir", g) in p["c16"]]
    if not sizes:
        return
    fig, axes = plt.subplots(1, 2, figsize=(11, 3.8))
    w = 0.16
    for ax, unit in zip(axes, ("qps", "gbs")):
        for i, (s, label, color, _) in enumerate(SCHEMES):
            for j, g in enumerate(sizes):
                x = j + (i - (len(SCHEMES) - 1) / 2) * w
                scale = g if unit == "gbs" else 1
                if s == "ypir":
                    ax.plot(x, (10 if unit == "gbs" else 3) * 1.3, marker="x", color=color, markersize=10, mew=3)
                    continue
                if (s, g) not in p["c16"]:
                    continue
                v, lo, hi = (q * scale for q in p["c16"][(s, g)])
                ax.bar(x, v, w, color=color, edgecolor="#555555", linewidth=0.6, yerr=[[v - lo], [hi - v]], capsize=2)
                if s == "altopir":
                    others = [p["c16"][(o, g)][0] for o, *_ in SCHEMES if o not in ("altopir", "ypir") and (o, g) in p["c16"]]
                    if others:
                        ax.text(x, v * 1.25, f"{p['c16'][(s, g)][0] / max(others):.1f}x", ha="center", va="bottom", color=RED, fontweight="bold")
        ax.set_yscale("log")
        ax.set_ylim(*((3, 2e4) if unit == "qps" else (10, 1e5)))
        ax.set_xticks(range(len(sizes)))
        ax.set_xticklabels([f"{g} GB" for g in sizes])
        style(ax, "Database Size", "Throughput (QPS)" if unit == "qps" else "Throughput (GB/s)")
        ax.grid(axis="x", visible=False)
    handles = [Line2D([0], [0], marker="x", color=c, linestyle="none", markersize=9, mew=3) if s == "ypir"
               else Patch(facecolor=c, edgecolor="#555555", linewidth=0.6) for s, _, c, _ in SCHEMES]
    fig.legend(handles, [l for _, l, _, _ in SCHEMES], loc="upper center", bbox_to_anchor=(0.5, 0.94), ncol=5, frameon=False)
    fig.suptitle(f"Throughput on {CORES} cores, 32 KB records (YPIR+SP runs only on one core)", y=1.0)
    fig.tight_layout(rect=(0, 0, 1, 0.95))
    save(fig, "throughput_comparison")


def database_size(p):
    fig, axes = plt.subplots(1, 2, figsize=(11, 3.8))
    drawn = False
    for ax, panel, title in zip(axes, ("c16", "c1"), (f"{CORES} cores", "1 core")):
        sizes = set()
        for s, label, color, marker in SCHEMES[::-1] + [("altopir_b1", "AltoPIR (B=1)", RED, "^")]:
            pts = sorted((g, q) for (o, g), q in p[panel].items() if o == s)
            if not pts:
                continue
            xs = [g for g, _ in pts]
            sizes.update(xs)
            ys = [q[0] for _, q in pts]
            dashed = s == "altopir_b1"
            ax.plot(xs, ys, marker=marker, color=color, linestyle="--" if dashed else "-", label=label,
                    markerfacecolor="white" if dashed else color)
            drawn = True
        if not sizes:
            ax.set_visible(False)
            continue
        ax.set_xscale("log", base=2)
        ax.set_yscale("log")
        ax.set_xticks(sorted(sizes))
        ax.set_xticklabels([str(g) for g in sorted(sizes)])
        ax.xaxis.set_minor_locator(matplotlib.ticker.NullLocator())
        ax.set_title(title)
        style(ax, "Database Size (GB)", "Throughput (QPS)")
    if not drawn:
        plt.close(fig)
        return
    handles = {}
    for ax in axes:
        handles.update({l: h for h, l in zip(*ax.get_legend_handles_labels())})
    order = ["AltoPIR", "AltoPIR (B=1)", "ReinsPIRe", "Valar", "InsPIRe", "YPIR+SP"]
    fig.legend([handles[k] for k in order if k in handles], [k for k in order if k in handles], loc="lower center",
               bbox_to_anchor=(0.5, 0.97), ncol=6, frameon=False)
    fig.tight_layout()
    save(fig, "throughput_vs_database_size")


def batch_size():
    fig, ax = plt.subplots(figsize=(5.5, 3.6))
    batches = set()
    for m, label, marker, color in (("amx", "AMX", "^", RED), ("vnni", "AVX-512 VNNI", "s", BLUE)):
        rows = altopir_rows(f"altopir_batch_{m}.txt")
        if not rows:
            continue
        xs = sorted(rows)
        batches.update(xs)
        ax.plot(xs, [b * 1e3 / rows[b] for b in xs], marker=marker, color=color, label=label)
    if not batches:
        plt.close(fig)
        return
    ax.set_xscale("log", base=2)
    ax.set_yscale("log")
    ax.set_xticks(sorted(batches))
    ax.set_xticklabels([str(b) for b in sorted(batches)])
    ax.xaxis.set_minor_locator(matplotlib.ticker.NullLocator())
    ax.set_title(f"AltoPIR on a 1 GB database, {CORES} cores")
    style(ax, "Batch Size", "Throughput (QPS)")
    ax.legend(frameon=False)
    fig.tight_layout()
    save(fig, "throughput_vs_batch_size")


def main():
    p = points()
    comparison(p)
    database_size(p)
    batch_size()
    summary.sort(key=lambda r: (r[0], r[1], r[2], r[3]))
    with open(os.path.join(OUT, "summary.csv"), "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["scheme", "database_gb", "cores", "batch", "plan", "median_ms", "qps"])
        w.writerows(summary)
    for panel, title in (("c16", f"{CORES} cores, best batch"), ("c1", "1 core, one query")):
        print(title)
        for (s, g), q in sorted(p[panel].items(), key=lambda kv: (kv[0][1], kv[0][0])):
            print(f"  {g:>3} GB  {s:<11} {q[0]:10.2f} QPS")


if __name__ == "__main__":
    main()
