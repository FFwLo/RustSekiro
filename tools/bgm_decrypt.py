"""Decrypts the music banks (sound/sm<area>.fsb) into extracted/sound for FMOD.

The game's music FSBs are encrypted the FMOD Ex way (each byte bit-reversed, then XOR the key,
repeating); the effect banks are not. The game hands FMOD the key: a UTF-16 string in
sekiro.exe. This finds it in the player's own exe by trying each UTF-16 text run as the key
against the FSB5 header (no key is kept in this project), then writes the plain banks next to the
sm<area>.fev / .itl the extractor unpacked (`sekiro-extract unpack ... '^/sound/sm[0-9][0-9]\\.(fev|itl)$'`).

  python tools/bgm_decrypt.py [<Sekiro install dir>]
"""
import glob
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'extracted')
DEFAULT = r'C:\Program Files (x86)\Steam\steamapps\common\Sekiro'
HEADER = b'FSB5\x01\x00\x00\x00'
REV = bytes(int('{:08b}'.format(b)[::-1], 2) for b in range(256))


def decrypt(data, key):
    out = bytearray(data.translate(REV))
    n = len(key)
    # XOR in 1 MiB blocks as big integers (whole key repeats per block: block size % n == 0).
    block = (1 << 20) // n * n
    stream = int.from_bytes(key * (block // n), 'little')
    for o in range(0, len(out), block):
        chunk = out[o:o + block]
        k = stream if len(chunk) == block else int.from_bytes((key * (len(chunk) // n + 1))[:len(chunk)], 'little')
        out[o:o + block] = (int.from_bytes(chunk, 'little') ^ k).to_bytes(len(chunk), 'little')
    return bytes(out)


def find_key(exe_path, sample):
    exe = open(exe_path, 'rb').read()
    head = sample[:len(HEADER)].translate(REV)
    for m in re.finditer(rb'(?:[\x21-\x7e]\x00){16,64}\x00\x00', exe):
        key = m.group()[:-2].decode('utf-16-le').encode('ascii')
        if all(head[i] ^ key[i % len(key)] == HEADER[i] for i in range(len(HEADER))):
            return key
    return None


def main():
    game = sys.argv[1] if len(sys.argv) > 1 else DEFAULT
    fsbs = sorted(glob.glob(os.path.join(game, 'sound', 'sm[0-9][0-9].fsb')))
    if not fsbs:
        sys.exit('no sound/sm*.fsb under %s' % game)
    key = None
    for f in fsbs:
        out = os.path.join(ROOT, 'sound', os.path.basename(f))
        data = open(f, 'rb').read()
        if data[:4] == b'FSB5':
            plain = data
        else:
            key = key or find_key(os.path.join(game, 'sekiro.exe'), data)
            if key is None:
                sys.exit('music key not found in sekiro.exe')
            plain = decrypt(data, key)
            assert plain[:len(HEADER)] == HEADER, f
        if os.path.exists(out) and open(out, 'rb').read(4) == b'FSB5' and os.path.getsize(out) == len(plain):
            print(os.path.basename(f), 'already plain')
            continue
        open(out, 'wb').write(plain)
        print(os.path.basename(f), len(plain), 'bytes')


if __name__ == '__main__':
    main()
