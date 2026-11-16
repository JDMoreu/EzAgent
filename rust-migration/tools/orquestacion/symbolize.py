"""Traduce RVAs de un exe sin símbolos a funciones de un exe equivalente con PDB.

Alinea las dos tablas de funciones (.pdata) por tamaños y resuelve con dbghelp.
Uso: python -I symbolize.py <exe_sin_simbolos> <exe_con_pdb> <rva> [<rva> ...]
"""

import ctypes
import ctypes.wintypes as wt
import difflib
import struct
import sys
from bisect import bisect_right


def pdata(path: str) -> list[tuple[int, int]]:
    data = open(path, "rb").read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    nsec = struct.unpack_from("<H", data, pe + 6)[0]
    opt = struct.unpack_from("<H", data, pe + 20)[0]
    table = pe + 24 + opt
    for i in range(nsec):
        off = table + 40 * i
        if data[off:off + 8].rstrip(b"\0") == b".pdata":
            vsize, _vaddr, rsize, rptr = struct.unpack_from("<IIII", data, off + 8)
            raw = data[rptr:rptr + min(vsize, rsize)]
            funcs = []
            for j in range(0, len(raw) - 11, 12):
                begin, end, _unwind = struct.unpack_from("<III", raw, j)
                if begin:
                    funcs.append((begin, end))
            return funcs
    raise SystemExit("sin .pdata")


plain, symbolic = sys.argv[1], sys.argv[2]
rvas = [int(value, 16) for value in sys.argv[3:]]
fa, fb = pdata(plain), pdata(symbolic)
sa = [end - begin for begin, end in fa]
sb = [end - begin for begin, end in fb]
matcher = difflib.SequenceMatcher(None, sa, sb, autojunk=False)
mapping = {}
for block in matcher.get_matching_blocks():
    for k in range(block.size):
        mapping[block.a + k] = block.b + k
begins = [begin for begin, _ in fa]

dbg = ctypes.WinDLL("dbghelp", use_last_error=True)


class SYMBOL_INFO(ctypes.Structure):
    _fields_ = [
        ("SizeOfStruct", wt.ULONG), ("TypeIndex", wt.ULONG), ("Reserved", ctypes.c_uint64 * 2),
        ("Index", wt.ULONG), ("Size", wt.ULONG), ("ModBase", ctypes.c_uint64), ("Flags", wt.ULONG),
        ("Value", ctypes.c_uint64), ("Address", ctypes.c_uint64), ("Register", wt.ULONG),
        ("Scope", wt.ULONG), ("Tag", wt.ULONG), ("NameLen", wt.ULONG), ("MaxNameLen", wt.ULONG),
        ("Name", ctypes.c_char * 1024),
    ]


class IMAGEHLP_LINE64(ctypes.Structure):
    _fields_ = [("SizeOfStruct", wt.DWORD), ("Key", ctypes.c_void_p), ("LineNumber", wt.DWORD),
                ("FileName", ctypes.c_char_p), ("Address", ctypes.c_uint64)]


dbg.SymLoadModuleEx.restype = ctypes.c_uint64
dbg.SymLoadModuleEx.argtypes = [wt.HANDLE, wt.HANDLE, ctypes.c_char_p, ctypes.c_char_p,
                                ctypes.c_uint64, wt.DWORD, ctypes.c_void_p, wt.DWORD]
dbg.SymFromAddr.argtypes = [wt.HANDLE, ctypes.c_uint64, ctypes.POINTER(ctypes.c_uint64), ctypes.POINTER(SYMBOL_INFO)]
dbg.SymGetLineFromAddr64.argtypes = [wt.HANDLE, ctypes.c_uint64, ctypes.POINTER(wt.DWORD), ctypes.POINTER(IMAGEHLP_LINE64)]
handle = wt.HANDLE(0x1234)
dbg.SymSetOptions(0x00000002 | 0x00000010 | 0x00000004)  # UNDNAME | DEFERRED | LOAD_LINES
import os
if not dbg.SymInitialize(handle, os.path.dirname(os.path.abspath(symbolic)).encode(), False):
    raise SystemExit(f"SymInitialize {ctypes.get_last_error()}")
BASE = 0x140000000
_d = open(symbolic, "rb").read(); _pe = struct.unpack_from("<I", _d, 0x3C)[0]; _size = struct.unpack_from("<I", _d, _pe + 24 + 56)[0]
if not dbg.SymLoadModuleEx(handle, None, symbolic.encode(), None, BASE, _size, None, 0):
    raise SystemExit(f"SymLoadModuleEx {ctypes.get_last_error()}")

for rva in rvas:
    index = bisect_right(begins, rva) - 1
    if index < 0 or rva >= fa[index][1]:
        print(f"0x{rva:x}: fuera de funciones")
        continue
    offset = rva - fa[index][0]
    target = mapping.get(index)
    if target is None:
        print(f"0x{rva:x}: función {index} sin pareja")
        continue
    mapped = fb[target][0] + offset
    symbol = SYMBOL_INFO()
    symbol.SizeOfStruct = 88
    symbol.MaxNameLen = 1000
    disp = ctypes.c_uint64(0)
    name = "?"
    if dbg.SymFromAddr(handle, BASE + mapped, ctypes.byref(disp), ctypes.byref(symbol)):
        name = symbol.Name.decode(errors="replace")
    line = IMAGEHLP_LINE64()
    line.SizeOfStruct = ctypes.sizeof(IMAGEHLP_LINE64)
    ldisp = wt.DWORD(0)
    where = ""
    if dbg.SymGetLineFromAddr64(handle, BASE + mapped, ctypes.byref(ldisp), ctypes.byref(line)):
        file = (line.FileName or b"").decode(errors="replace")
        where = f"  [{file.split('crates')[-1] if 'crates' in file else file}:{line.LineNumber}]"
    print(f"0x{rva:x} -> {name}{where}")
