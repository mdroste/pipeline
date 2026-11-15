//! A local message pipe with an administrator owner and explicit client rights.
//! GENERIC_WRITE would also grant creation of a competing server instance.
//! Both endpoints authenticate the other's kernel process ID before any RPC.
use super::identity::{self, Handle};
use std::{
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*, Security::Authorization::*, Security::*, Storage::FileSystem::*,
    System::Pipes::*,
};

const LIMIT: usize = 4096;
const CLIENT_ACCESS: u32 = FILE_GENERIC_READ | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES;
pub(super) struct Pipe(pub Handle);
impl Pipe {
    pub fn server(name: &str, user: &str) -> Result<Self, String> {
        let sddl = identity::wide(&format!(
            "O:BAG:BAD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x{CLIENT_ACCESS:x};;;{user})S:(ML;;NW;;;ME)"
        ));
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(identity::error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let raw = unsafe {
            CreateNamedPipeW(
                identity::wide(name).as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE
                    | PIPE_READMODE_MESSAGE
                    | PIPE_NOWAIT
                    | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                LIMIT as u32,
                LIMIT as u32,
                0,
                &attributes,
            )
        };
        let result = Handle::new(raw).map(Self);
        unsafe {
            LocalFree(descriptor);
        }
        result
    }
    pub fn accept(&self, parent: u32) -> Result<bool, String> {
        // In NOWAIT mode a true result means the instance became available,
        // not that a client connected. Only PIPE_CONNECTED admits the peer.
        if unsafe { ConnectNamedPipe(self.0 .0, null_mut()) } != 0 {
            return Ok(false);
        }
        match unsafe { GetLastError() } {
            ERROR_PIPE_LISTENING => return Ok(false),
            ERROR_PIPE_CONNECTED => {}
            _ => return Err(identity::error()),
        }
        let mut pid = 0;
        if unsafe { GetNamedPipeClientProcessId(self.0 .0, &mut pid) } == 0 {
            return Err(identity::error());
        }
        if pid != parent {
            return Err("Unexpected firewall guard client".into());
        }
        Ok(true)
    }
    pub fn client(name: &str, helper: u32) -> Result<Self, String> {
        let raw = unsafe {
            CreateFileW(
                identity::wide(name).as_ptr(),
                CLIENT_ACCESS,
                0,
                null(),
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                null_mut(),
            )
        };
        let pipe = Self(Handle::new(raw)?);
        let mut pid = 0;
        if unsafe { GetNamedPipeServerProcessId(pipe.0 .0, &mut pid) } == 0 || pid != helper {
            return Err("Unexpected firewall guard server".into());
        }
        let mut owner = null_mut();
        let mut descriptor = null_mut();
        let code = unsafe {
            GetSecurityInfo(
                pipe.0 .0,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            )
        };
        if code != 0 {
            return Err(format!("Cannot authenticate firewall pipe: {code}"));
        }
        let owner = unsafe { identity::sid_text(owner) };
        unsafe {
            LocalFree(descriptor);
        }
        if owner? != "S-1-5-32-544" {
            return Err("Firewall pipe is not owned by administrators".into());
        }
        let mode = PIPE_READMODE_MESSAGE | PIPE_NOWAIT;
        if unsafe { SetNamedPipeHandleState(pipe.0 .0, &mode, null(), null()) } == 0 {
            return Err(identity::error());
        }
        Ok(pipe)
    }
    pub fn read(&self) -> Result<Option<Vec<u8>>, String> {
        let mut bytes = vec![0; LIMIT];
        let mut read = 0;
        if unsafe {
            ReadFile(
                self.0 .0,
                bytes.as_mut_ptr(),
                LIMIT as u32,
                &mut read,
                null_mut(),
            )
        } == 0
        {
            return match unsafe { GetLastError() } {
                ERROR_NO_DATA => Ok(None),
                ERROR_MORE_DATA => Err("Oversized firewall guard message".into()),
                _ => Err(identity::error()),
            };
        }
        if read == 0 {
            return Err("Firewall guard connection closed".into());
        }
        bytes.truncate(read as usize);
        Ok(Some(bytes))
    }
    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > LIMIT {
            return Err("Invalid firewall message length".into());
        }
        let mut written = 0;
        if unsafe {
            WriteFile(
                self.0 .0,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                null_mut(),
            )
        } == 0
            || written as usize != bytes.len()
        {
            return Err("Firewall guard connection could not accept a message".into());
        }
        Ok(())
    }
    pub fn receive(&self, timeout: Duration) -> Result<Vec<u8>, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(bytes) = self.read()? {
                return Ok(bytes);
            }
            if Instant::now() >= deadline {
                return Err("Firewall guard did not respond".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
