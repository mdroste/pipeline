"""Read-only host diagnostic of AFD's device security descriptor."""
import ctypes as c
from ctypes import wintypes as w
import json

nt = c.WinDLL('ntdll')
adv = c.WinDLL('advapi32', use_last_error=True)
k = c.WinDLL('kernel32', use_last_error=True)

class Unicode(c.Structure):
    _fields_ = [('Length', w.USHORT), ('MaximumLength', w.USHORT), ('Buffer', w.LPWSTR)]

class Attributes(c.Structure):
    _fields_ = [('Length', w.ULONG), ('RootDirectory', w.HANDLE), ('ObjectName', c.POINTER(Unicode)), ('Attributes', w.ULONG), ('SecurityDescriptor', w.LPVOID), ('SecurityQualityOfService', w.LPVOID)]

class IoStatus(c.Structure):
    _fields_ = [('Status', c.c_void_p), ('Information', c.c_size_t)]

nt.NtOpenFile.argtypes = [c.POINTER(w.HANDLE), w.ULONG, c.POINTER(Attributes), c.POINTER(IoStatus), w.ULONG, w.ULONG]
nt.NtOpenFile.restype = w.LONG
nt.NtQuerySecurityObject.argtypes = [w.HANDLE, w.ULONG, w.LPVOID, w.ULONG, c.POINTER(w.ULONG)]
nt.NtQuerySecurityObject.restype = w.LONG
adv.ConvertSecurityDescriptorToStringSecurityDescriptorW.argtypes = [w.LPVOID, w.DWORD, w.DWORD, c.POINTER(w.LPWSTR), c.POINTER(w.ULONG)]
adv.ConvertSecurityDescriptorToStringSecurityDescriptorW.restype = w.BOOL
k.CloseHandle.argtypes = [w.HANDLE]
k.LocalFree.argtypes = [w.LPVOID]
for name in [r'\Device\Afd', r'\Device\Tcp', r'\Device\Udp']:
    buf = c.create_unicode_buffer(name)
    uni = Unicode(len(name)*2, (len(name)+1)*2, c.cast(buf, w.LPWSTR))
    attrs = Attributes(c.sizeof(Attributes), None, c.pointer(uni), 0x40, None, None)
    handle, io = w.HANDLE(), IoStatus()
    result = nt.NtOpenFile(c.byref(handle), 0x20000, c.byref(attrs), c.byref(io), 7, 0)
    row = dict(device=name, open_status=hex(result & 0xffffffff))
    if result >= 0:
        try:
            size = w.ULONG()
            nt.NtQuerySecurityObject(handle, 7, None, 0, c.byref(size))
            sd = c.create_string_buffer(size.value)
            result = nt.NtQuerySecurityObject(handle, 7, sd, size.value, c.byref(size))
            row['query_status'] = hex(result & 0xffffffff)
            if result >= 0:
                text = w.LPWSTR()
                if adv.ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, 1, 7, c.byref(text), None):
                    row['sddl'] = text.value
                    k.LocalFree(text)
                else:
                    row['sddl_error'] = c.get_last_error()
        finally:
            k.CloseHandle(handle)
    print(json.dumps(row))
