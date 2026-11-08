#!/usr/bin/env python3
"""Ventanas *layered* de 64 bits para la isla de EzAgent.

Maquinaria Win32 de ventanas con transparencia real por píxel: la clase
``LayeredWindow`` (UpdateLayeredWindow + DIB propio) es la base sobre la que se
dibuja la muesca del notch. También da la geometría del escritorio
(``work_area``, ``taskbar_rect``).

Por qué existe este módulo: el código anterior declaraba ``ctypes.WinDLL`` sin
``argtypes``/``restype``. En x64 eso trunca cualquier HANDLE a 32 bits y, con
extensión de signo, ``GetModuleHandleW`` devolvía ``0xffffffffdb4e0000`` en vez
de ``0x7ff6db4e0000``: un puntero basura que hacía que ``RegisterClassExW`` y
``CreateWindowExW`` reventaran con *access violation* (una por hilo de la
muesca). Aquí TODAS las funciones llevan prototipo, y la ventana *layered* es
una clase reutilizable con gestión de DIB protegida por cerrojo (la carrera de
liberación también provocaba AV en ``memmove``).

Solo depende de ctypes y numpy: no importa la UI de la isla para no crear
ciclos.

Basado en Dicta (MIT).
"""

import ctypes
import os
import threading
import time

import numpy as np

# ------------------------------------------------------------------ geometría
# Unidades lógicas (se escalan por DPI): 1 u = 1 px a 96 ppp.
EDGE_MARGIN = 18               # separación al borde del área de trabajo

# Tokens de DESIGN.md
_ACCENT_DARK = (91, 157, 255)      # #5B9DFF
_ACCENT_LIGHT = (0, 102, 204)      # #0066CC

# Degradado de la onda: teal -> azul -> violeta (igual que la isla).

_IS_WIN = os.name == "nt"

# ------------------------------------------------------------ Win32 prototipos
if _IS_WIN:
    from ctypes import wintypes

    _u32 = ctypes.WinDLL("user32", use_last_error=True)
    _g32 = ctypes.WinDLL("gdi32", use_last_error=True)
    _k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    try:
        _sh32 = ctypes.WinDLL("shell32", use_last_error=True)
    except OSError:                                   # pragma: no cover
        _sh32 = None

    WNDPROC = ctypes.WINFUNCTYPE(ctypes.c_ssize_t, ctypes.c_void_p,
                                 ctypes.c_uint, ctypes.c_size_t,
                                 ctypes.c_ssize_t)
    HGDIOBJ = ctypes.c_void_p
    HWND = ctypes.c_void_p

    class _WNDCLASSEXW(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.UINT),
                    ("style", wintypes.UINT),
                    ("lpfnWndProc", WNDPROC),
                    ("cbClsExtra", ctypes.c_int),
                    ("cbWndExtra", ctypes.c_int),
                    ("hInstance", ctypes.c_void_p),
                    ("hIcon", ctypes.c_void_p),
                    ("hCursor", ctypes.c_void_p),
                    ("hbrBackground", ctypes.c_void_p),
                    ("lpszMenuName", ctypes.c_wchar_p),
                    ("lpszClassName", ctypes.c_wchar_p),
                    ("hIconSm", ctypes.c_void_p)]

    class _POINT(ctypes.Structure):
        _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]

    class _SIZE(ctypes.Structure):
        _fields_ = [("cx", ctypes.c_long), ("cy", ctypes.c_long)]

    class _RECT(ctypes.Structure):
        _fields_ = [("left", ctypes.c_long), ("top", ctypes.c_long),
                    ("right", ctypes.c_long), ("bottom", ctypes.c_long)]

    class _MONITORINFO(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("rcMonitor", _RECT),
                    ("rcWork", _RECT), ("dwFlags", wintypes.DWORD)]

    class _BLENDFUNCTION(ctypes.Structure):
        _fields_ = [("BlendOp", ctypes.c_byte), ("BlendFlags", ctypes.c_byte),
                    ("SrcConstantAlpha", ctypes.c_byte),
                    ("AlphaFormat", ctypes.c_byte)]

    class _BITMAPINFOHEADER(ctypes.Structure):
        _fields_ = [("biSize", ctypes.c_ulong), ("biWidth", ctypes.c_long),
                    ("biHeight", ctypes.c_long),
                    ("biPlanes", ctypes.c_ushort),
                    ("biBitCount", ctypes.c_ushort),
                    ("biCompression", ctypes.c_ulong),
                    ("biSizeImage", ctypes.c_ulong),
                    ("biXPelsPerMeter", ctypes.c_long),
                    ("biYPelsPerMeter", ctypes.c_long),
                    ("biClrUsed", ctypes.c_ulong),
                    ("biClrImportant", ctypes.c_ulong)]

    class _CREATESTRUCTW(ctypes.Structure):
        _fields_ = [("lpCreateParams", ctypes.c_void_p),
                    ("hInstance", ctypes.c_void_p),
                    ("hMenu", ctypes.c_void_p),
                    ("hwndParent", ctypes.c_void_p),
                    ("cy", ctypes.c_int), ("cx", ctypes.c_int),
                    ("y", ctypes.c_int), ("x", ctypes.c_int),
                    ("style", ctypes.c_long),
                    ("lpszName", ctypes.c_wchar_p),
                    ("lpszClass", ctypes.c_wchar_p),
                    ("dwExStyle", ctypes.c_ulong)]

    class _TRACKMOUSEEVENT(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("dwFlags", wintypes.DWORD),
                    ("hwndTrack", ctypes.c_void_p),
                    ("dwHoverTime", wintypes.DWORD)]

    class _MSLLHOOKSTRUCT(ctypes.Structure):
        _fields_ = [("pt", _POINT), ("mouseData", wintypes.DWORD),
                    ("flags", wintypes.DWORD), ("time", wintypes.DWORD),
                    ("dwExtraInfo", ctypes.c_size_t)]

    _HOOKPROC = ctypes.WINFUNCTYPE(
        ctypes.c_ssize_t, ctypes.c_int, ctypes.c_size_t, ctypes.c_ssize_t
    )

    def _prototypes():
        u, g, k = _u32, _g32, _k32
        k.GetModuleHandleW.argtypes = [ctypes.c_wchar_p]
        k.GetModuleHandleW.restype = ctypes.c_void_p

        u.RegisterClassExW.argtypes = [ctypes.POINTER(_WNDCLASSEXW)]
        u.RegisterClassExW.restype = ctypes.c_ushort
        u.UnregisterClassW.argtypes = [ctypes.c_wchar_p, ctypes.c_void_p]
        u.CreateWindowExW.argtypes = [
            wintypes.DWORD, wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD,
            ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
            ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p,
            ctypes.c_void_p]
        u.CreateWindowExW.restype = ctypes.c_void_p
        u.DefWindowProcW.argtypes = [ctypes.c_void_p, ctypes.c_uint,
                                     ctypes.c_size_t, ctypes.c_ssize_t]
        u.DefWindowProcW.restype = ctypes.c_ssize_t
        u.DestroyWindow.argtypes = [ctypes.c_void_p]
        u.DestroyWindow.restype = wintypes.BOOL
        u.IsWindow.argtypes = [ctypes.c_void_p]
        u.IsWindow.restype = wintypes.BOOL
        u.ShowWindow.argtypes = [ctypes.c_void_p, ctypes.c_int]
        u.ShowWindow.restype = wintypes.BOOL
        u.SetWindowPos.argtypes = [
            ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
            ctypes.c_int, ctypes.c_int, wintypes.UINT]
        u.SetWindowPos.restype = wintypes.BOOL
        u.LoadCursorW.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        u.LoadCursorW.restype = ctypes.c_void_p
        u.SetCursor.argtypes = [ctypes.c_void_p]
        u.SetCursor.restype = ctypes.c_void_p
        u.GetCursorPos.argtypes = [ctypes.POINTER(_POINT)]
        u.GetCursorPos.restype = wintypes.BOOL
        u.GetAsyncKeyState.argtypes = [ctypes.c_int]
        u.GetAsyncKeyState.restype = ctypes.c_short
        u.SetCapture.argtypes = [ctypes.c_void_p]
        u.SetCapture.restype = ctypes.c_void_p
        u.ReleaseCapture.argtypes = []
        u.ReleaseCapture.restype = wintypes.BOOL
        u.TrackMouseEvent.argtypes = [ctypes.POINTER(_TRACKMOUSEEVENT)]
        u.TrackMouseEvent.restype = wintypes.BOOL
        u.PostMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint,
                                   ctypes.c_size_t, ctypes.c_ssize_t]
        u.PostMessageW.restype = wintypes.BOOL
        u.PeekMessageW.argtypes = [ctypes.c_void_p, ctypes.c_void_p,
                                   wintypes.UINT, wintypes.UINT,
                                   wintypes.UINT]
        u.PeekMessageW.restype = wintypes.BOOL
        u.TranslateMessage.argtypes = [ctypes.c_void_p]
        u.TranslateMessage.restype = wintypes.BOOL
        u.DispatchMessageW.argtypes = [ctypes.c_void_p]
        u.DispatchMessageW.restype = ctypes.c_ssize_t
        u.SystemParametersInfoW.argtypes = [
            wintypes.UINT, wintypes.UINT, ctypes.c_void_p, wintypes.UINT]
        u.SystemParametersInfoW.restype = wintypes.BOOL
        u.GetSystemMetrics.argtypes = [ctypes.c_int]
        u.GetSystemMetrics.restype = ctypes.c_int
        u.FindWindowW.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p]
        u.FindWindowW.restype = ctypes.c_void_p
        u.MonitorFromPoint.argtypes = [_POINT, wintypes.DWORD]
        u.MonitorFromPoint.restype = ctypes.c_void_p
        u.GetMonitorInfoW.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(_MONITORINFO)
        ]
        u.GetMonitorInfoW.restype = wintypes.BOOL
        u.GetWindowRect.argtypes = [ctypes.c_void_p, ctypes.POINTER(_RECT)]
        u.GetWindowRect.restype = wintypes.BOOL
        u.SetForegroundWindow.argtypes = [ctypes.c_void_p]
        u.SetForegroundWindow.restype = wintypes.BOOL
        u.GetForegroundWindow.argtypes = []
        u.GetForegroundWindow.restype = ctypes.c_void_p
        u.SetFocus.argtypes = [ctypes.c_void_p]
        u.SetFocus.restype = ctypes.c_void_p
        u.GetFocus.argtypes = []
        u.GetFocus.restype = ctypes.c_void_p
        u.GetWindowLongPtrW.argtypes = [ctypes.c_void_p, ctypes.c_int]
        u.GetWindowLongPtrW.restype = ctypes.c_ssize_t
        u.SetWindowLongPtrW.argtypes = [
            ctypes.c_void_p, ctypes.c_int, ctypes.c_ssize_t
        ]
        u.SetWindowLongPtrW.restype = ctypes.c_ssize_t
        u.OpenClipboard.argtypes = [ctypes.c_void_p]
        u.OpenClipboard.restype = wintypes.BOOL
        u.CloseClipboard.argtypes = []
        u.CloseClipboard.restype = wintypes.BOOL
        u.IsClipboardFormatAvailable.argtypes = [wintypes.UINT]
        u.IsClipboardFormatAvailable.restype = wintypes.BOOL
        u.GetClipboardData.argtypes = [wintypes.UINT]
        u.GetClipboardData.restype = ctypes.c_void_p
        k.GlobalLock.argtypes = [ctypes.c_void_p]
        k.GlobalLock.restype = ctypes.c_void_p
        k.GlobalUnlock.argtypes = [ctypes.c_void_p]
        k.GlobalUnlock.restype = wintypes.BOOL
        u.SetWindowsHookExW.argtypes = [
            ctypes.c_int, _HOOKPROC, ctypes.c_void_p, wintypes.DWORD
        ]
        u.SetWindowsHookExW.restype = ctypes.c_void_p
        u.UnhookWindowsHookEx.argtypes = [ctypes.c_void_p]
        u.UnhookWindowsHookEx.restype = wintypes.BOOL
        u.CallNextHookEx.argtypes = [
            ctypes.c_void_p, ctypes.c_int, ctypes.c_size_t, ctypes.c_ssize_t
        ]
        u.CallNextHookEx.restype = ctypes.c_ssize_t
        u.UpdateLayeredWindow.argtypes = [
            ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(_POINT),
            ctypes.POINTER(_SIZE), ctypes.c_void_p,
            ctypes.POINTER(_POINT), wintypes.DWORD,
            ctypes.POINTER(_BLENDFUNCTION), wintypes.DWORD]
        u.UpdateLayeredWindow.restype = wintypes.BOOL
        if hasattr(u, "GetDpiForSystem"):
            u.GetDpiForSystem.argtypes = []
            u.GetDpiForSystem.restype = wintypes.UINT

        g.CreateCompatibleDC.argtypes = [ctypes.c_void_p]
        g.CreateCompatibleDC.restype = ctypes.c_void_p
        g.CreateDIBSection.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(_BITMAPINFOHEADER), wintypes.UINT,
            ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD]
        g.CreateDIBSection.restype = ctypes.c_void_p
        g.SelectObject.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        g.SelectObject.restype = ctypes.c_void_p
        g.DeleteObject.argtypes = [ctypes.c_void_p]
        g.DeleteObject.restype = wintypes.BOOL
        g.DeleteDC.argtypes = [ctypes.c_void_p]
        g.DeleteDC.restype = wintypes.BOOL

        if _sh32 is not None:
            _sh32.DragAcceptFiles.argtypes = [ctypes.c_void_p, wintypes.BOOL]
            _sh32.DragAcceptFiles.restype = None
            _sh32.DragQueryFileW.argtypes = [ctypes.c_void_p, wintypes.UINT,
                                             wintypes.LPWSTR, wintypes.UINT]
            _sh32.DragQueryFileW.restype = wintypes.UINT
            _sh32.DragFinish.argtypes = [ctypes.c_void_p]
            _sh32.DragFinish.restype = None

    try:
        _prototypes()
        _PROTO_OK = True
    except Exception:                                  # pragma: no cover
        _PROTO_OK = False
else:                                                  # pragma: no cover
    _PROTO_OK = False

# HWND_TOPMOST = (HWND)-1
_HWND_TOPMOST = ctypes.c_void_p(-1) if _IS_WIN else None
_SWP_NOSIZE = 0x0001
_SWP_NOZORDER = 0x0004
_SWP_NOACTIVATE = 0x0010


def available():
    """True si se puede crear chrome flotante en este proceso."""
    return bool(_IS_WIN and _PROTO_OK)


def enable_dpi_awareness() -> None:
    """Per-Monitor V2 antes de crear Tk/Win32 (spec 4.4/11.5).

    Si el sistema no expone ``SetProcessDpiAwarenessContext``, cae a
    ``SetProcessDpiAwareness``. Debe llamarse una sola vez, al arrancar.
    """
    if not _IS_WIN:
        return
    try:
        fn = _u32.SetProcessDpiAwarenessContext
        fn.argtypes = [ctypes.c_void_p]
        fn.restype = wintypes.BOOL
        fn(ctypes.c_void_p(-4))  # DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
        return
    except Exception:
        pass
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    except Exception:
        pass


# ---------------------------------------------------------------- utilidades
def _instance():
    return _k32.GetModuleHandleW(None)


def work_area():
    """(left, top, right, bottom) del área de trabajo, ya sin la barra de
    tareas. SPI_GETWORKAREA = 0x0030 (el código anterior usaba 0x0048, que no
    es esa constante: la llamada fallaba y la muesca se colaba bajo la barra)."""
    if _IS_WIN:
        r = _RECT()
        if _u32.SystemParametersInfoW(0x0030, 0, ctypes.byref(r), 0):
            return (int(r.left), int(r.top), int(r.right), int(r.bottom))
    return (0, 0, 1920, 1080)


def monitor_rect(x: int | None = None, y: int | None = None):
    """(left, top, right, bottom) del monitor activo, incluida la barra.

    La spec 4.1 pide ``rcMonitor``, no ``rcWork``: la isla cuelga del borde
    físico superior. Se usa el monitor bajo el cursor al arrancar y
    ``MONITOR_DEFAULTTONEAREST`` como respaldo.
    """
    if not _IS_WIN:
        return (0, 0, 1920, 1080)
    try:
        if x is None or y is None:
            p = _POINT()
            _u32.GetCursorPos(ctypes.byref(p))
            x, y = int(p.x), int(p.y)
        hmon = _u32.MonitorFromPoint(_POINT(int(x), int(y)), 2)
        info = _MONITORINFO()
        info.cbSize = ctypes.sizeof(_MONITORINFO)
        if hmon and _u32.GetMonitorInfoW(hmon, ctypes.byref(info)):
            r = info.rcMonitor
            return (int(r.left), int(r.top), int(r.right), int(r.bottom))
    except Exception:
        pass
    wa = work_area()
    return wa


def taskbar_rect():
    """(left, top, right, bottom) de la barra de tareas, o None.

    Hace falta porque con la barra auto-oculta SPI_GETWORKAREA devuelve la
    pantalla COMPLETA: sin esto, la ventana se colocaría justo donde la barra
    aparece al asomar el ratón.
    """
    if not _IS_WIN:
        return None
    try:
        hwnd = _u32.FindWindowW("Shell_TrayWnd", None)
        if not hwnd:
            return None
        r = _RECT()
        if not _u32.GetWindowRect(hwnd, ctypes.byref(r)):
            return None
        return (int(r.left), int(r.top), int(r.right), int(r.bottom))
    except Exception:
        return None


def clamp_pos(x, y, size):
    """Mantiene la ventana dentro del área de trabajo.

    Sin esto, una posición guardada (o un arrastre) puede dejar la ventana
    fuera de la pantalla o debajo de la barra de tareas, y el usuario se queda
    sin poder recuperarla.
    """
    wa = work_area()
    m = max(0, int(EDGE_MARGIN / 2))
    x = min(max(int(x), wa[0] + m), max(wa[0] + m, wa[2] - int(size) - m))
    y = min(max(int(y), wa[1] + m), max(wa[1] + m, wa[3] - int(size) - m))
    tb = taskbar_rect()
    if tb and tb[1] > wa[1] + (wa[3] - wa[1]) // 2:
        y = min(y, tb[1] - int(size) - m)       # por encima de la barra
    return (x, y)


def _lbutton_down():
    """Estado REAL del botón izquierdo (no la cola de eventos)."""
    try:
        return bool(_u32.GetAsyncKeyState(0x01) & 0x8000)     # VK_LBUTTON
    except Exception:
        return True


# ----------------------------------------------------------- ventana layered
_WINDOWS = {}                       # wm_id -> instancia
_BY_HWND = {}                       # hwnd   -> instancia
_WM_SEQ = [0]
_CLASS_ATOM = [None]
_CLASS_LOCK = threading.Lock()
_CLASS_NAME = "ezagent-layered-window"

_WM_CLOSE = 0x0010
_WM_PAINT = 0x000F
_WM_SETFOCUS = 0x0007
_WM_KILLFOCUS = 0x0008
_WM_KEYDOWN = 0x0100
_WM_CHAR = 0x0102
_WM_MOUSEACTIVATE = 0x0021
_WM_NCCREATE = 0x0081
_WM_MOUSEMOVE = 0x0200
_WM_LBUTTONDOWN = 0x0201
_WM_LBUTTONUP = 0x0202
_WM_RBUTTONUP = 0x0205
_WM_MOUSEWHEEL = 0x020A
_WM_MOUSELEAVE = 0x02A3
_WM_NCHITTEST = 0x0084
_WM_DPICHANGED = 0x02E0
_WM_DROPFILES = 0x0233
_HTTRANSPARENT = -1
_MA_NOACTIVATE = 3
_MA_ACTIVATE = 1
_IDC_HAND = 32649
_WS_EX_NOACTIVATE = 0x08000000
_GWL_EXSTYLE = -20
_CF_UNICODETEXT = 13
_WH_MOUSE_LL = 14


def _ensure_class():
    with _CLASS_LOCK:
        if _CLASS_ATOM[0]:
            return True
        wc = _WNDCLASSEXW()
        wc.cbSize = ctypes.sizeof(_WNDCLASSEXW)
        wc.style = 0x0002 | 0x0001                # CS_HREDRAW | CS_VREDRAW
        wc.lpfnWndProc = _wndproc
        wc.hInstance = _instance()
        wc.hCursor = _u32.LoadCursorW(None, ctypes.c_void_p(32512))  # IDC_ARROW
        wc.lpszClassName = _CLASS_NAME
        atom = _u32.RegisterClassExW(ctypes.byref(wc))
        if not atom:
            err = ctypes.get_last_error()
            if err == 1410:                       # ERROR_CLASS_ALREADY_EXISTS
                _CLASS_ATOM[0] = 1
                return True
            return False
        _CLASS_ATOM[0] = atom
        return True


def _wndproc_impl(hwnd, msg, wparam, lparam):
    """Único WndProc de la clase; despacha a la instancia dueña del HWND."""
    try:
        if msg == _WM_NCCREATE:
            cs = ctypes.cast(lparam, ctypes.POINTER(_CREATESTRUCTW)).contents
            wid = int(cs.lpCreateParams or 0)
            win = _WINDOWS.get(wid)
            if win is not None:
                win.hwnd = int(hwnd or 0)
                _BY_HWND[win.hwnd] = win
        else:
            win = _BY_HWND.get(int(hwnd or 0))
            if win is not None:
                out = win._on_message(msg, wparam, lparam)
                if out is not None:
                    return out
    except Exception:
        pass
    try:
        return _u32.DefWindowProcW(hwnd, msg, wparam, lparam)
    except Exception:
        return 0


# La instancia viva del callback: si se libera, Windows llama a memoria muerta.
_wndproc = WNDPROC(_wndproc_impl) if _IS_WIN else None


class LayeredWindow:
    """Ventana WS_EX_LAYERED con alpha por píxel y bucle de dibujo propio.

    Subclases: sobrescribir ``render()`` (devuelve numpy BGRA premultiplicado)
    y, si hace falta, ``_on_message``. ``click_through=True`` deja pasar todos
    los clics; si no, las zonas con alpha < 8 se declaran HTTRANSPARENT.
    """

    FPS_ACTIVE = 30

    def __init__(self, w, h, x=0, y=0, click_through=False, title="ezagent"):
        self.w = int(w)
        self.h = int(h)
        self.x = int(x)
        self.y = int(y)
        self.click_through = bool(click_through)
        self.title = title
        self.hwnd = 0
        self._wid = _WM_SEQ[0] = _WM_SEQ[0] + 1
        self._stop = threading.Event()
        self._thread = None
        self._lock = threading.RLock()          # protege DIB y hwnd
        self._dc = None
        self._bmp = None
        self._old = None
        self._bits = None
        self._frame = None                      # último BGRA (para hit-test)
        self._img = None                        # último PIL RGBA (depuración)
        self._dib_size = (0, 0)                 # tamaño real del DIB actual
        self._visible = False
        self._keyboard = False
        self._previous_foreground = 0
        self._restore_foreground_at = 0.0
        self._restore_foreground_hwnd = 0
        self._mouse_hook = None
        self._hook_proc = None
        self._dirty = True
        self._last_paint = 0.0
        self._interval = 1.0 / 20.0
        self._hover = None
        self._hits = []                         # [(x1,y1,x2,y2,accion)]
        self._dragging = False
        self._pointer_active = False
        self._drag_off = (0, 0)
        self._drag_moved = False
        self._tracking = False
        self._created = threading.Event()
        self._ok = False
        self.max_height = 0                     # alto máximo reservado (px físicos)
        self.diag = ""                          # motivo real si algo falla

    # ---- API pública
    def start(self):
        if not available():
            return False
        # Una ventana que se arranca es una ventana que se quiere ver: sin esto
        # quedaba creada pero oculta y el bucle de dibujo no pintaba nada.
        self._visible = True
        self._dirty = True
        if self._thread is not None and self._thread.is_alive():
            self.show()
            return True
        self._stop.clear()
        self._thread = threading.Thread(target=self._run, daemon=True,
                                        name=f"ezagent-{self.title}")
        self._thread.start()
        self._created.wait(timeout=4.0)
        return self._ok

    def stop(self, destroy=True):
        self._stop.set()
        t = self._thread
        if t is not None and t.is_alive() and t is not threading.current_thread():
            t.join(timeout=2.0)
        self._thread = None
        if destroy:
            with self._lock:
                hwnd = self.hwnd
            if hwnd:
                try:
                    _u32.PostMessageW(hwnd, _WM_CLOSE, 0, 0)
                except Exception:
                    pass
        self._visible = False

    def show(self):
        self._visible = True
        self._dirty = True
        with self._lock:
            hwnd = self.hwnd
        if hwnd:
            try:
                _u32.ShowWindow(hwnd, 4)          # SW_SHOWNOACTIVATE
                # Sin SWP_NOZORDER: el notch se reafirma por encima de la
                # barra de tareas (también es topmost) al mostrarse.
                _u32.SetWindowPos(hwnd, _HWND_TOPMOST, self.x, self.y,
                                  self.w, self.h,
                                  _SWP_NOACTIVATE)
            except Exception:
                pass

    def hide(self):
        self._visible = False
        self._dirty = True
        with self._lock:
            hwnd = self.hwnd
        if hwnd:
            try:
                _u32.ShowWindow(hwnd, 0)          # SW_HIDE
            except Exception:
                pass

    def is_visible(self):
        return bool(self._visible)

    def set_pos(self, x, y):
        self.x, self.y = int(x), int(y)
        self._dirty = True
        with self._lock:
            hwnd = self.hwnd
        if hwnd:
            try:
                # Sin SWP_NOZORDER: cada movimiento reafirma el topmost para
                # que la píldora siga visible aunque esté sobre la taskbar.
                _u32.SetWindowPos(hwnd, _HWND_TOPMOST, self.x, self.y, 0, 0,
                                  _SWP_NOSIZE | _SWP_NOACTIVATE)
            except Exception:
                pass

    def get_pos(self):
        return (self.x, self.y)

    def resize(self, w, h):
        """Cambia el tamaño (recrea el DIB dentro del hilo de dibujo)."""
        if (int(w), int(h)) == (self.w, self.h):
            return
        self.w, self.h = int(w), int(h)
        self._pending_resize = True
        self._dirty = True

    def invalidate(self):
        self._dirty = True

    # ---- a sobrescribir
    def render(self):                                  # pragma: no cover
        return np.zeros((self.h, self.w, 4), np.uint8)

    def on_click(self, action):                        # pragma: no cover
        pass

    def on_files(self, paths):                         # pragma: no cover
        pass

    # ---- teclado (solo mientras se edita un campo de texto)
    def on_key(self, vk: int, ctrl: bool) -> bool:
        return False

    def on_char(self, char: str) -> bool:
        return False

    def on_paste(self, text: str) -> bool:
        return False

    def on_focus_lost(self) -> None:
        return None

    def on_global_wheel(self, x: int, y: int, delta: int) -> bool:
        """Hook de rueda global; solo lo usa la isla con el menú abierto."""
        return False

    def _install_mouse_hook(self) -> None:
        if not _IS_WIN or self._mouse_hook is not None:
            return
        try:
            self._hook_proc = _HOOKPROC(self._on_mouse_hook)
            self._mouse_hook = _u32.SetWindowsHookExW(
                _WH_MOUSE_LL,
                self._hook_proc,
                _k32.GetModuleHandleW(None),
                0,
            )
        except Exception:
            self._mouse_hook = None
            self._hook_proc = None

    def _uninstall_mouse_hook(self) -> None:
        hook = self._mouse_hook
        self._mouse_hook = None
        self._hook_proc = None
        if hook and _IS_WIN:
            try:
                _u32.UnhookWindowsHookEx(hook)
            except Exception:
                pass

    def _on_mouse_hook(self, code, wparam, lparam):
        try:
            if code >= 0 and int(wparam) == _WM_MOUSEWHEEL:
                data = ctypes.cast(
                    lparam, ctypes.POINTER(_MSLLHOOKSTRUCT)
                ).contents
                delta = ctypes.c_short(
                    (int(data.mouseData) >> 16) & 0xFFFF
                ).value
                if self.on_global_wheel(int(data.pt.x), int(data.pt.y), delta):
                    return 1
        except Exception:
            pass
        return _u32.CallNextHookEx(None, code, wparam, lparam)

    def set_keyboard_enabled(self, enabled: bool) -> None:
        """Activa el teclado temporalmente retirando WS_EX_NOACTIVATE."""
        was_enabled = self._keyboard
        self._keyboard = bool(enabled)
        hwnd = int(self.hwnd or 0)
        if not hwnd or not _IS_WIN:
            return
        try:
            if enabled and not was_enabled:
                self._restore_foreground_at = 0.0
                self._restore_foreground_hwnd = 0
                try:
                    self._previous_foreground = int(
                        _u32.GetForegroundWindow() or 0
                    )
                except Exception:
                    self._previous_foreground = 0
            style = int(_u32.GetWindowLongPtrW(hwnd, _GWL_EXSTYLE))
            if enabled:
                style &= ~_WS_EX_NOACTIVATE
            else:
                style |= _WS_EX_NOACTIVATE
            _u32.SetWindowLongPtrW(hwnd, _GWL_EXSTYLE, style)
            if enabled:
                _u32.ShowWindow(hwnd, 4)      # SW_SHOWNOACTIVATE
                _u32.SetForegroundWindow(hwnd)
                _u32.SetFocus(hwnd)
            else:
                previous = int(self._previous_foreground or 0)
                self._previous_foreground = 0
                if previous and previous != hwnd and _u32.IsWindow(previous):
                    # No devolver el foco dentro del WM_KEYDOWN: el WM_CHAR
                    # del Enter podría acabar en la app anterior. Se aplaza
                    # unos milisegundos y solo si la isla sigue delante.
                    self._restore_foreground_hwnd = previous
                    self._restore_foreground_at = time.monotonic() + 0.08
        except Exception:
            pass

    def _restore_focus_if_due(self) -> None:
        at = self._restore_foreground_at
        if not at or time.monotonic() < at:
            return
        self._restore_foreground_at = 0.0
        previous = self._restore_foreground_hwnd
        self._restore_foreground_hwnd = 0
        hwnd = int(self.hwnd or 0)
        if not previous or not hwnd or previous == hwnd:
            return
        try:
            if int(_u32.GetForegroundWindow() or 0) != hwnd:
                return
            if _u32.IsWindow(previous):
                _u32.SetForegroundWindow(previous)
        except Exception:
            pass

    def _clipboard_text(self) -> str:
        if not _IS_WIN:
            return ""
        try:
            if not _u32.IsClipboardFormatAvailable(_CF_UNICODETEXT):
                return ""
            if not _u32.OpenClipboard(int(self.hwnd or 0)):
                return ""
            try:
                handle = _u32.GetClipboardData(_CF_UNICODETEXT)
                if not handle:
                    return ""
                ptr = _k32.GlobalLock(handle)
                if not ptr:
                    return ""
                try:
                    return ctypes.wstring_at(ptr)
                finally:
                    _k32.GlobalUnlock(handle)
            finally:
                _u32.CloseClipboard()
        except Exception:
            return ""

    # ---- puntero interactivo (menú, sliders, rueda)
    # Los hooks reciben coordenadas de cliente en píxeles físicos. Si
    # ``on_pointer_down`` devuelve True, la ventana no inicia el arrastre.
    def on_pointer_down(self, x: int, y: int) -> bool:
        return False

    def on_pointer_move(self, x: int, y: int) -> bool:
        return False

    def on_pointer_up(self, x: int, y: int) -> bool:
        return False

    def on_wheel(self, x: int, y: int, delta: int) -> bool:
        return False

    def cursor_hand(self, x: int, y: int) -> bool:
        return False

    def on_dpi_changed(self, dpi: int) -> None:
        return None

    # ---- arrastre de la ventana (la isla puede convertirse en pill)
    def clamp_position(self, x: int, y: int) -> tuple[int, int]:
        return clamp_pos(x, y, self.w)

    def on_window_drag_start(self, x: int, y: int) -> None:
        return None

    def on_window_drag_move(self, x: int, y: int) -> None:
        return None

    def on_window_drag_end(self, x: int, y: int) -> None:
        return None

    # ---- bucle
    def _run(self):
        try:
            if not _ensure_class():
                return
            hwnd = _u32.CreateWindowExW(
                0x00080000 |          # WS_EX_LAYERED
                0x00000200 |          # WS_EX_TOOLWINDOW
                0x00000008 |          # WS_EX_TOPMOST
                0x08000000 |          # WS_EX_NOACTIVATE
                (0x00000020 if self.click_through else 0),   # WS_EX_TRANSPARENT
                _CLASS_NAME, self.title, 0x80000000,          # WS_POPUP
                self.x, self.y, self.w, self.h, None, None,
                _instance(), ctypes.c_void_p(self._wid))
            if not hwnd:
                return
            self.hwnd = int(hwnd)
            _BY_HWND[self.hwnd] = self
            _WINDOWS[self._wid] = self      # el WndProc busca aqui su instancia
            if _sh32 is not None:
                try:
                    _sh32.DragAcceptFiles(hwnd, True)
                except Exception:
                    pass
            if not self._dib_create():
                self.diag = f"CreateDIBSection fallo ({ctypes.get_last_error()})"
                return
            self._ok = True
            self._install_mouse_hook()
            if self._keyboard:
                self.set_keyboard_enabled(True)
        except Exception:
            self._ok = False
            return
        finally:
            self._created.set()

        if self._visible:
            _u32.ShowWindow(self.hwnd, 4)
            # Reafirma TOPMOST sin SWP_NOZORDER: recien creada, la ventana
            # puede quedarse por debajo de otra topmost y parecer invisible.
            _u32.SetWindowPos(self.hwnd, _HWND_TOPMOST, self.x, self.y,
                              self.w, self.h, _SWP_NOACTIVATE)
        else:
            _u32.ShowWindow(self.hwnd, 0)

        try:
            import ctypes as _ct
            if hasattr(_ct, "windll") and hasattr(_ct.windll, "winmm"):
                _ct.windll.winmm.timeBeginPeriod(1)
        except Exception:
            pass

        try:
            while not self._stop.is_set():
                self._pump()
                self._restore_focus_if_due()
                now = time.monotonic()
                if self._visible and (self._dirty
                                      or now - self._last_paint >= self._interval):
                    self._paint(now)
                delay = 1.0 / 60.0 - (time.monotonic() - now)
                if delay > 0.001:
                    time.sleep(delay)
        finally:
            self._uninstall_mouse_hook()
            self._dib_release()
            try:
                import ctypes as _ct
                _ct.windll.winmm.timeEndPeriod(1)
            except Exception:
                pass
            hwnd, self.hwnd = self.hwnd, 0
            _BY_HWND.pop(hwnd, None)
            _WINDOWS.pop(self._wid, None)
            if hwnd and _u32.IsWindow(hwnd):
                try:
                    _u32.DestroyWindow(hwnd)
                except Exception:
                    pass

    _MSG = None

    def _pump(self):
        """Extrae y despacha mensajes de ESTE hilo (no bloqueante)."""
        if self._MSG is None:
            class _M(ctypes.Structure):
                _fields_ = [("hwnd", ctypes.c_void_p), ("message", ctypes.c_uint),
                            ("wParam", ctypes.c_size_t),
                            ("lParam", ctypes.c_ssize_t),
                            ("time", ctypes.c_ulong),
                            ("pt", _POINT)]
            LayeredWindow._MSG = _M
        msg = self._MSG()
        while _u32.PeekMessageW(ctypes.byref(msg), None, 0, 0, 1):   # PM_REMOVE
            _u32.TranslateMessage(ctypes.byref(msg))
            _u32.DispatchMessageW(ctypes.byref(msg))

    def _dib_create(self, w=None, h=None):
        """Crea el DIB del tamaño pedido (por defecto, el de la ventana)."""
        w = int(self.w if w is None else w)
        h = int(self.h if h is None else h)
        with self._lock:
            self._dib_release_locked()
            bi = _BITMAPINFOHEADER()
            bi.biSize = ctypes.sizeof(_BITMAPINFOHEADER)
            bi.biWidth, bi.biHeight = w, -h
            bi.biPlanes, bi.biBitCount, bi.biCompression = 1, 32, 0
            self._dc = _g32.CreateCompatibleDC(None)
            self._bits = ctypes.c_void_p()
            self._bmp = _g32.CreateDIBSection(
                self._dc, ctypes.byref(bi), 0, ctypes.byref(self._bits),
                None, 0)
            if not self._bmp or not self._bits:
                self._dib_release_locked()
                return False
            self._old = _g32.SelectObject(self._dc, self._bmp)
            self._dib_size = (w, h)
            return True

    def _dib_release_locked(self):
        try:
            if self._dc and self._old:
                _g32.SelectObject(self._dc, self._old)
            if self._bmp:
                _g32.DeleteObject(self._bmp)
            if self._dc:
                _g32.DeleteDC(self._dc)
        except Exception:
            pass
        self._dc = self._bmp = self._old = None
        # OJO: dejar _bits apuntando a un DIB liberado era la carrera que
        # reventaba en memmove (access violation en el hilo de dibujo).
        self._bits = None
        self._frame = None
        self._dib_size = (0, 0)

    def _dib_release(self):
        with self._lock:
            self._dib_release_locked()

    def _paint(self, now):
        try:
            frame = self.render()
        except Exception as e:
            # Silenciar esto hacía que la ventana se quedara vacía sin ninguna
            # pista de por qué.
            self.diag = f"render: {type(e).__name__}: {e}"
            frame = None
        if frame is None:
            self._dirty = False
            self._last_paint = now
            return
        frame = np.ascontiguousarray(frame, dtype=np.uint8)
        if frame.ndim != 3 or frame.shape[2] != 4:
            self._dirty = False
            self._last_paint = now
            return
        fh, fw = int(frame.shape[0]), int(frame.shape[1])
        hwnd = self.hwnd
        with self._lock:
            dib_w, dib_h = self._dib_size
            window_resized = (fw, fh) != (self.w, self.h)
            # La ventana del notch reserva un único DIB del alto máximo y
            # solo cambia el área visible. Recrearlo en cada fotograma de la
            # animación de altura provocaría reallocaciones y access violations.
            target_h = max(fh, int(getattr(self, "max_height", 0) or 0))
            if not self._bits or dib_w != fw or dib_h < fh:
                if not self._dib_create(fw, max(target_h, fh)):
                    self.diag = ("CreateDIBSection fallo "
                                 f"({ctypes.get_last_error()})")
                    self._dirty = False
                    return
                dib_w, dib_h = self._dib_size
            bits = self._bits
            dc = self._dc
            if dib_w == fw:
                ctypes.memmove(bits, frame.ctypes.data, frame.nbytes)
            else:  # cambio de ancho por DPI: copiar fila a fila
                row_bytes = fw * 4
                base = int(bits)
                src = int(frame.ctypes.data)
                for row in range(fh):
                    ctypes.memmove(
                        base + row * dib_w * 4,
                        src + row * row_bytes,
                        row_bytes,
                    )
            self.w, self.h = fw, fh
            self._frame = frame
        if window_resized and hwnd:
            _u32.SetWindowPos(hwnd, _HWND_TOPMOST, self.x, self.y, fw, fh,
                              _SWP_NOACTIVATE)
        pt_src = _POINT(0, 0)
        pt_dst = _POINT(self.x, self.y)
        size = _SIZE(fw, fh)
        blend = _BLENDFUNCTION(0, 0, 255, 1)
        if not _u32.UpdateLayeredWindow(hwnd, None, ctypes.byref(pt_dst),
                                       ctypes.byref(size), dc,
                                       ctypes.byref(pt_src), 0,
                                       ctypes.byref(blend), 2):
            # Un fallo aqui es invisible salvo por esto: la ventana existe pero
            # se queda vacia y parece que "no funciona el chrome".
            self.diag = ("UpdateLayeredWindow fallo "
                         f"({ctypes.get_last_error()})")
        else:
            self.diag = ""
        self._dirty = False
        self._last_paint = now

    def _alpha_at(self, x, y):
        """Alpha del último fotograma en coordenadas de cliente."""
        f = self._frame
        if f is None or not (0 <= x < self.w and 0 <= y < self.h):
            return 0
        try:
            return int(f[y, x, 3])
        except Exception:
            return 0

    def _hit_at(self, x, y):
        for (x1, y1, x2, y2, act) in self._hits:
            if x1 <= x <= x2 and y1 <= y <= y2:
                return act
        return None

    def _screen_cursor(self):
        p = _POINT()
        _u32.GetCursorPos(ctypes.byref(p))
        return int(p.x), int(p.y)

    def _set_cursor(self, hand: bool) -> None:
        """Cursor mano sobre zonas interactivas; flecha en el resto."""
        try:
            ident = _IDC_HAND if hand else 32512  # IDC_ARROW
            _u32.SetCursor(_u32.LoadCursorW(None, ctypes.c_void_p(ident)))
        except Exception:
            pass

    # ---- mensajes (se ejecutan en el hilo de la ventana)
    def _on_message(self, msg, wparam, lparam):
        if msg in (_WM_NCCREATE,):
            return None
        hwnd = self.hwnd
        if msg == _WM_MOUSEACTIVATE:
            # Sin edición la ventana no se activa: el usuario sigue escribiendo
            # en su app. Con un campo de texto abierto sí necesita el foco.
            return _MA_ACTIVATE if self._keyboard else _MA_NOACTIVATE
        if msg == _WM_NCHITTEST and not self.click_through:
            cx, cy = self._screen_cursor()
            lx, ly = cx - self.x, cy - self.y
            if self._alpha_at(lx, ly) <= 0:
                return _HTTRANSPARENT
            return 1                                  # HTCLIENT
        if msg == _WM_PAINT:
            return 0
        if msg == _WM_DPICHANGED:
            try:
                self.on_dpi_changed(int(wparam) & 0xFFFF)
            except Exception:
                pass
            # El rect sugerido por Windows se aplica en el hilo de render.
            return 0
        if msg == _WM_KEYDOWN and self._keyboard:
            vk = int(wparam or 0) & 0xFFFF
            ctrl = bool(_u32.GetAsyncKeyState(0x11) & 0x8000)
            if vk == 86 and ctrl:          # Ctrl+V
                try:
                    if self.on_paste(self._clipboard_text()):
                        return 0
                except Exception:
                    pass
            try:
                if self.on_key(vk, ctrl):
                    return 0
            except Exception:
                pass
            return None
        if msg == _WM_CHAR and self._keyboard:
            char = chr(int(wparam or 0) & 0xFFFF)
            try:
                if self.on_char(char):
                    return 0
            except Exception:
                pass
            return None
        if msg == _WM_KILLFOCUS and self._keyboard:
            try:
                self.on_focus_lost()
            except Exception:
                pass
            return 0
        if msg == _WM_MOUSEWHEEL and not self.click_through:
            # wParam: delta en la palabra alta; lParam: cursor en pantalla.
            delta = ctypes.c_short((wparam >> 16) & 0xFFFF).value
            sx = ctypes.c_short(lparam & 0xFFFF).value
            sy = ctypes.c_short((lparam >> 16) & 0xFFFF).value
            x, y = sx - self.x, sy - self.y
            try:
                if self.on_wheel(x, y, delta):
                    return 0
            except Exception:
                pass
            return None
        if msg == _WM_DROPFILES and _sh32 is not None:
            try:
                n = _sh32.DragQueryFileW(wparam, 0xFFFFFFFF, None, 0)
                paths = []
                for i in range(n):
                    ln = _sh32.DragQueryFileW(wparam, i, None, 0)
                    buf = ctypes.create_unicode_buffer(ln + 1)
                    _sh32.DragQueryFileW(wparam, i, buf, ln + 1)
                    paths.append(buf.value)
                _sh32.DragFinish(wparam)
                if paths:
                    threading.Thread(target=self.on_files, args=(paths,),
                                     daemon=True).start()
            except Exception:
                pass
            return 0
        if msg == _WM_MOUSEMOVE:
            x = ctypes.c_short(lparam & 0xFFFF).value
            y = ctypes.c_short((lparam >> 16) & 0xFFFF).value
            if self._pointer_active:
                try:
                    self.on_pointer_move(x, y)
                except Exception:
                    pass
                return 0
            if self._dragging and not _lbutton_down():
                # Se perdio el WM_LBUTTONUP (p. ej. lo consumio otro proceso):
                # sin esto el arrastre se queda pegado y la ventana sigue al
                # cursor sin que nadie pulse nada.
                self._dragging = False
                self._drag_moved = False
                try:
                    _u32.ReleaseCapture()
                except Exception:
                    pass
                try:
                    self.on_window_drag_end(self.x, self.y)
                except Exception:
                    pass
            if self._dragging:
                sx, sy = self._screen_cursor()
                nx = sx - self._drag_off[0]
                ny = sy - self._drag_off[1]
                if abs(nx - self.x) > 2 or abs(ny - self.y) > 2:
                    self._drag_moved = True
                nx, ny = self.clamp_position(nx, ny)
                self.set_pos(nx, ny)
                if self._drag_moved:
                    try:
                        self.on_window_drag_move(self.x, self.y)
                    except Exception:
                        pass
            elif not self.click_through:
                if not self._tracking:
                    tme = _TRACKMOUSEEVENT(ctypes.sizeof(_TRACKMOUSEEVENT),
                                           0x00000002, hwnd, 0)  # TME_LEAVE
                    _u32.TrackMouseEvent(ctypes.byref(tme))
                    self._tracking = True
                try:
                    hand = bool(self.cursor_hand(x, y))
                except Exception:
                    hand = False
                self._set_cursor(hand)
                act = self._hit_at(x, y)
                if act != self._hover:
                    self._hover = act
                    self._dirty = True
            return 0
        if msg == _WM_MOUSELEAVE:
            self._tracking = False
            if not self._pointer_active and self._hover is not None:
                self._hover = None
                self._dirty = True
            return 0
        if msg == _WM_LBUTTONDOWN and not self.click_through:
            x = ctypes.c_short(lparam & 0xFFFF).value
            y = ctypes.c_short((lparam >> 16) & 0xFFFF).value
            if self._alpha_at(x, y) > 0:
                try:
                    handled = bool(self.on_pointer_down(x, y))
                except Exception:
                    handled = False
                if handled:
                    self._pointer_active = True
                    _u32.SetCapture(hwnd)
                    return 0
                sx, sy = self._screen_cursor()
                self._dragging = True
                self._drag_moved = False
                self._drag_off = (sx - self.x, sy - self.y)
                _u32.SetCapture(hwnd)
                try:
                    self.on_window_drag_start(self.x, self.y)
                except Exception:
                    pass
            return 0
        if msg == _WM_LBUTTONUP and not self.click_through:
            x = ctypes.c_short(lparam & 0xFFFF).value
            y = ctypes.c_short((lparam >> 16) & 0xFFFF).value
            if self._pointer_active:
                self._pointer_active = False
                try:
                    self.on_pointer_up(x, y)
                except Exception:
                    pass
                try:
                    _u32.ReleaseCapture()
                except Exception:
                    pass
                return 0
            was_drag = self._drag_moved
            self._dragging = False
            self._drag_moved = False
            try:
                _u32.ReleaseCapture()
            except Exception:
                pass
            if not was_drag:
                act = self._hit_at(x, y)
                if act:
                    threading.Thread(target=self.on_click, args=(act,),
                                     daemon=True).start()
            else:
                try:
                    self.on_window_drag_end(self.x, self.y)
                except Exception:
                    pass
                if self._moved_cb:
                    self._moved_cb(self.x, self.y)
            return 0
        if msg == _WM_RBUTTONUP and not self.click_through:
            if self._right_cb:
                threading.Thread(target=self._right_cb, daemon=True).start()
            return 0
        if msg == _WM_CLOSE:
            self._stop.set()
            return 0
        return None

    _moved_cb = None
    _right_cb = None
    _pending_resize = False


# --------------------------------------------------------------------- isla
def prepare():
    """Comprueba que el chrome puede crearse; devuelve un motivo si no."""
    if not _IS_WIN:
        return "solo Windows"
    if not _PROTO_OK:
        return "no se pudieron declarar los prototipos de Win32"
    if not _ensure_class():
        return "no se pudo registrar la clase de ventana"
    return ""
