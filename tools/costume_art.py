"""The Change Form pictures (src/form.rs): menu/hi/menu_costume_<protector id>.tpf ->
extracted/menu_costume/<protector id>.png, from the user's own Sekiro data.

Each TPF holds one texture: the memory's picture the game's 02_206_CostumeSelect screen shows
(103000 Original Memory: Wolf, 303000 Shura, 403000 Ashina, 503000 Tengu: the forms' leg rows of
EquipParamProtector, named as the memories).

Needs: `sekiro-extract unpack <Sekiro dir> extracted '^/menu/hi/menu_costume_[0-9]+\\.tpf'`.
Usage: python tools/costume_art.py
"""
import glob
import os
import struct
import subprocess

ROOT = os.path.join(os.path.dirname(__file__), '..')
EXTRACT = os.environ.get('SEKIRO_EXTRACT') or os.path.join(ROOT, 'tools/sekiro-extract/sekiro-extract.exe')


def main():
    out = os.path.join(ROOT, 'extracted/menu_costume')
    os.makedirs(out, exist_ok=True)
    done = 0
    for tpf in sorted(glob.glob(os.path.join(ROOT, 'extracted/menu/hi/menu_costume_*.tpf'))):
        cid = os.path.basename(tpf)[len('menu_costume_'):-len('.tpf')]
        d = open(tpf, 'rb').read()
        doff, dsz = struct.unpack_from('<Ii', d, 0x10)
        dds = os.path.join(out, f'{cid}.dds')
        open(dds, 'wb').write(d[doff:doff + dsz])
        if subprocess.run([EXTRACT, 'dds2png', dds, os.path.join(out, f'{cid}.png')], stdout=subprocess.DEVNULL).returncode == 0:
            done += 1
        os.remove(dds)
    print(f'{done} costume pictures -> {out}')


if __name__ == '__main__':
    main()
