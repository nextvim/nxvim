#!/usr/bin/env python3
import random
import sys

seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
operations = int(sys.argv[2]) if len(sys.argv) > 2 else 500
rng = random.Random(seed)
tree_len = 0

for index in range(operations):
    kind = rng.randrange(8)
    if kind == 0:
        print("push", rng.randrange(10000))
        tree_len += 1
    elif kind == 1:
        count = rng.randrange(1, 9)
        print("append", *(rng.randrange(10000) for _ in range(count)))
        tree_len += count
    elif kind == 2:
        target = rng.randrange(tree_len + 3)
        print("seek", target, "L" if rng.randrange(2) == 0 else "R")
    elif kind == 3:
        start = rng.randrange(tree_len + 1)
        end = rng.randrange(start, tree_len + 1)
        print("slice", start, end)
    elif kind == 4:
        print("map_put", rng.randrange(128), rng.randrange(10000))
    elif kind == 5:
        print("map_remove", rng.randrange(128))
    elif kind == 6:
        print("set_add", rng.randrange(128))
    else:
        print("set_remove", rng.randrange(128))
    if index % 31 == 0:
        print("emit")
print("emit")
