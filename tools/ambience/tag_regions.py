"""Mark the takes of a pack that belong to one country (`region`: de or uk) from regions.csv.

usage: python3 -I tag_regions.py <regions.csv> <pack dir>
The game plays a region's takes only on its maps (German on right-hand-traffic maps, British
on left-hand-traffic ones) and untagged takes everywhere.
"""
import csv, json, os, sys


def tag(regions_csv, pack_dir):
    regions = {r["source"]: r["region"] for r in csv.DictReader(open(regions_csv, encoding="utf-8"))}
    path = os.path.join(pack_dir, "pack.json")
    pack = json.load(open(path, encoding="utf-8"))
    n = 0
    for t in pack["takes"]:
        r = regions.get(t.get("source", ""))
        if r:
            t["region"] = r
            n += 1
        else:
            t.pop("region", None)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(pack, f, ensure_ascii=False, indent=1)
    return n


if __name__ == "__main__":
    print("tagged", tag(sys.argv[1], sys.argv[2]), "takes")
