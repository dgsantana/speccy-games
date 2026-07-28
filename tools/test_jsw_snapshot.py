"""Checks the snapshot loader against the snapshots in z80/.

Run: python3 tools/test_jsw_snapshot.py
Skips cleanly when a snapshot is not present, because none is committed.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from jsw_snapshot import load_z80, word

JSW2 = "z80/Jet_Set_Willy_II_The_Final_Frontier_1985_Software_Projects.z80"


def main():
    if not os.path.exists(JSW2):
        print(f"skipped: {JSW2} is not here")
        return
    memory = load_z80(JSW2)
    assert len(memory) == 65536, len(memory)
    # The room table is found through the word at 7E69h, and is BAFDh here.
    assert word(memory, 0x7E69) == 0xBAFD, hex(word(memory, 0x7E69))
    # The game names itself, typo and all, at 6480h.
    signature = bytes(memory[0x6480:0x6493]).decode("latin-1")
    assert signature == "THE FINAL FROINTIER", repr(signature)
    print("snapshot loader ok")


main()
