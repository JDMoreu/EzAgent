"""Vuelca las pilas de todos los hilos de un proceso (solo lectura).

Uso: python -I stackdump.py <pid> [max_frames]
Suspende cada hilo un instante, recorre su pila con dbghelp y lo reanuda.
"""

import ctypes
import ctypes.wintypes as wt
import sys

pid = int(sys.argv[1])
max_frames = int(sys.argv[2]) if len(sys.argv) > 2 else 40

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
dbg = ctypes.WinDLL("dbghelp", use_last_error=True)

PROCESS_ALL = 0x0400 | 0x0010  # QUERY_INFORMATION | VM_READ
THREAD_ACCESS = 0x0008 | 0x0002 | 0x0040 | 0x0800  # GET_CONTEXT|SUSPEND_RESUME|QUERY_INFORMATION|QUERY_LIMITED
TH32CS_SNAPTHREAD = 0x4


class THREADENTRY32(ctypes.Structure):
    _fields_ = [
        ("dwSize", wt.DWORD), ("cntUsage", wt.DWORD), ("th32ThreadID", wt.DWORD),
        ("th32OwnerProcessID", wt.DWORD), ("tpBasePri", wt.LONG), ("tpDeltaPri", wt.LONG),
        ("dwFlags", wt.DWORD),
    ]


class ADDRESS64(ctypes.Structure):
    _fields_ = [("Offset", ctypes.c_uint64), ("Segment", wt.WORD), ("Mode", wt.DWORD)]


class STACKFRAME64(ctypes.Structure):
    _fields_ = [
        ("AddrPC", ADDRESS64), ("AddrReturn", ADDRESS64), ("AddrFrame", ADDRESS64),
        ("AddrStack", ADDRESS64), ("AddrBStore", ADDRESS64), ("FuncTableEntry", ctypes.c_void_p),
        ("Params", ctypes.c_uint64 * 4), ("Far", wt.BOOL), ("Virtual", wt.BOOL),
        ("Reserved", ctypes.c_uint64 * 3), ("KdHelp", ctypes.c_byte * 256),
    ]


class SYMBOL_INFO(ctypes.Structure):
    _fields_ = [
        ("SizeOfStruct", wt.ULONG), ("TypeIndex", wt.ULONG), ("Reserved", ctypes.c_uint64 * 2),
        ("Index", wt.ULONG), ("Size", wt.ULONG), ("ModBase", ctypes.c_uint64), ("Flags", wt.ULONG),
        ("Value", ctypes.c_uint64), ("Address", ctypes.c_uint64), ("Register", wt.ULONG),
        ("Scope", wt.ULONG), ("Tag", wt.ULONG), ("NameLen", wt.ULONG), ("MaxNameLen", wt.ULONG),
        ("Name", ctypes.c_char * 512),
    ]


class IMAGEHLP_MODULE64(ctypes.Structure):
    _fields_ = [
        ("SizeOfStruct", wt.DWORD), ("BaseOfImage", ctypes.c_uint64), ("ImageSize", wt.DWORD),
        ("TimeDateStamp", wt.DWORD), ("CheckSum", wt.DWORD), ("NumSyms", wt.DWORD),
        ("SymType", wt.DWORD), ("ModuleName", ctypes.c_char * 32), ("ImageName", ctypes.c_char * 256),
        ("LoadedImageName", ctypes.c_char * 256),
    ]


k32.OpenProcess.restype = wt.HANDLE
k32.OpenThread.restype = wt.HANDLE
k32.CreateToolhelp32Snapshot.restype = wt.HANDLE
dbg.SymFunctionTableAccess64.restype = ctypes.c_void_p
dbg.SymGetModuleBase64.restype = ctypes.c_uint64
dbg.SymFunctionTableAccess64.argtypes = [wt.HANDLE, ctypes.c_uint64]
dbg.SymGetModuleBase64.argtypes = [wt.HANDLE, ctypes.c_uint64]
FTA = ctypes.cast(dbg.SymFunctionTableAccess64, ctypes.c_void_p)
GMB = ctypes.cast(dbg.SymGetModuleBase64, ctypes.c_void_p)
dbg.StackWalk64.argtypes = [
    wt.DWORD, wt.HANDLE, wt.HANDLE, ctypes.POINTER(STACKFRAME64), ctypes.c_void_p,
    ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p,
]
dbg.SymFromAddr.argtypes = [wt.HANDLE, ctypes.c_uint64, ctypes.POINTER(ctypes.c_uint64), ctypes.POINTER(SYMBOL_INFO)]
dbg.SymGetModuleInfo64.argtypes = [wt.HANDLE, ctypes.c_uint64, ctypes.POINTER(IMAGEHLP_MODULE64)]

try:
    GetThreadDescription = k32.GetThreadDescription
    GetThreadDescription.argtypes = [wt.HANDLE, ctypes.POINTER(ctypes.c_wchar_p)]
except AttributeError:
    GetThreadDescription = None

process = k32.OpenProcess(PROCESS_ALL, False, pid)
if not process:
    sys.exit(f"OpenProcess falló: {ctypes.get_last_error()}")
dbg.SymSetOptions(0x00000002 | 0x00000010)  # UNDNAME | DEFERRED_LOADS
if not dbg.SymInitialize(process, None, True):
    sys.exit(f"SymInitialize falló: {ctypes.get_last_error()}")

snap = k32.CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
entry = THREADENTRY32()
entry.dwSize = ctypes.sizeof(THREADENTRY32)
tids = []
ok = k32.Thread32First(snap, ctypes.byref(entry))
while ok:
    if entry.th32OwnerProcessID == pid:
        tids.append(entry.th32ThreadID)
    ok = k32.Thread32Next(snap, ctypes.byref(entry))
k32.CloseHandle(snap)


def describe(addr: int) -> str:
    module = IMAGEHLP_MODULE64()
    module.SizeOfStruct = ctypes.sizeof(IMAGEHLP_MODULE64)
    mod = "?"
    base = 0
    if dbg.SymGetModuleInfo64(process, addr, ctypes.byref(module)):
        mod = module.ModuleName.decode(errors="replace")
        base = module.BaseOfImage
    symbol = SYMBOL_INFO()
    symbol.SizeOfStruct = 88
    symbol.MaxNameLen = 500
    disp = ctypes.c_uint64(0)
    if mod.lower().startswith("ezagent") and base:
        return f"{mod}+0x{addr - base:x}"
    if dbg.SymFromAddr(process, addr, ctypes.byref(disp), ctypes.byref(symbol)):
        return f"{mod}!{symbol.Name.decode(errors='replace')}+0x{disp.value:x}"
    return f"{mod}+0x{addr - base:x}" if base else f"0x{addr:x}"


CONTEXT_SIZE = 1232
for tid in tids:
    thread = k32.OpenThread(THREAD_ACCESS, False, tid)
    if not thread:
        print(f"== hilo {tid}: OpenThread falló")
        continue
    name = ""
    if GetThreadDescription is not None:
        text = ctypes.c_wchar_p()
        if GetThreadDescription(thread, ctypes.byref(text)) >= 0 and text.value:
            name = text.value
    raw = (ctypes.c_byte * (CONTEXT_SIZE + 16))()
    addr = (ctypes.addressof(raw) + 15) & ~15
    ctypes.c_uint32.from_address(addr + 0x30).value = 0x10000B  # CONTEXT_FULL (AMD64)
    k32.SuspendThread(thread)
    try:
        if not k32.GetThreadContext(thread, ctypes.c_void_p(addr)):
            print(f"== hilo {tid} {name}: GetThreadContext falló {ctypes.get_last_error()}")
            continue
        rip = ctypes.c_uint64.from_address(addr + 0xF8).value
        rsp = ctypes.c_uint64.from_address(addr + 0x98).value
        rbp = ctypes.c_uint64.from_address(addr + 0xA0).value
        frame = STACKFRAME64()
        frame.AddrPC.Offset, frame.AddrPC.Mode = rip, 3
        frame.AddrStack.Offset, frame.AddrStack.Mode = rsp, 3
        frame.AddrFrame.Offset, frame.AddrFrame.Mode = rbp, 3
        lines = []
        for _ in range(max_frames):
            if not dbg.StackWalk64(0x8664, process, thread, ctypes.byref(frame), ctypes.c_void_p(addr),
                                   None, FTA, GMB, None):
                break
            if frame.AddrPC.Offset == 0:
                break
            lines.append(describe(frame.AddrPC.Offset))
    finally:
        k32.ResumeThread(thread)
        k32.CloseHandle(thread)
    print(f"== hilo {tid} «{name}»")
    for line in lines:
        print(f"   {line}")
dbg.SymCleanup(process)
k32.CloseHandle(process)
