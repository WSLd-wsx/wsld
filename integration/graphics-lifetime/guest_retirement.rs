// SPDX-License-Identifier: MIT
//! Exact-device retirement, including loss of its distro or original WSL VM.
//! Derived from the verified GuestRetirement adapter; no process is killed.
use std::{ffi::{c_void, OsString}, fs::File, io::{self, Read, Write}, marker::PhantomData,
    os::windows::{ffi::OsStringExt, process::CommandExt, io::AsRawHandle}, path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio}, rc::Rc, time::{Duration, Instant}};
#[path = "lifetime_policy.rs"]
mod policy;

#[link(name = "kernel32")]
extern "system" {
    fn GetSystemDirectoryW(buffer: *mut u16, length: u32) -> u32;
    fn PeekNamedPipe(pipe: *mut c_void, buffer: *mut c_void, size: u32,
        read: *mut u32, available: *mut u32, remaining: *mut u32) -> i32;
}
extern "C" { fn wsx_direct_bootstrap_vm_exited(bootstrap: *mut c_void) -> i32; }

fn system_wsl() -> io::Result<PathBuf> {
    #[cfg(test)]
    if let Some(path)=std::env::var_os("WSX_TEST_WSL") {return Ok(PathBuf::from(path));}
    let mut buffer = vec![0u16; 32768];
    let count = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if count == 0 { return Err(io::Error::last_os_error()); }
    if count >= buffer.len() { return Err(io::ErrorKind::InvalidData.into()); }
    let mut path = PathBuf::from(OsString::from_wide(&buffer[..count]));
    path.push("wsl.exe"); Ok(path)
}

#[cfg(test)]
#[path = "guest_retirement_tests.rs"]
mod tests;

pub struct GuestRetirement {
    executable: PathBuf, distribution: String, instance: String, log: File,
    child: Option<Child>, result: Option<ExitStatus>, attempted: bool,
    bootstrap: *mut c_void, vm_exited: bool, system_retire: bool, helper_failure_since: Option<Instant>,
    seen_running: bool, distro_stopped: bool, query: Option<Child>,
    query_started: Instant, next_query: Instant, next_vm_query: Instant,
    system_child: Option<Child>, system_ready: bool, system_started: Instant,
    normal_child: Option<Child>, normal_ready: bool, normal_started: Instant,
    thread: PhantomData<Rc<()>>,
}

impl GuestRetirement {
    /// # Safety
    /// The distribution, instance and retained bootstrap must belong to the
    /// exact constructed HDV lifetime. Call only on its original Owner thread.
    /// The bootstrap outlives all observation/retirement calls. Guest interfaces
    /// and WSL's system environment are trusted; failure must preserve ownership.
    pub unsafe fn new(distribution: String, instance: String, log: File,
        bootstrap: *mut c_void, guest_protocol: bool) -> io::Result<Self> {
        let guid = instance.trim_matches(|c| c == '{' || c == '}');
        if bootstrap.is_null() || distribution.is_empty() || distribution.contains('\0') || guid.len() != 36 ||
            !guid.bytes().enumerate().all(|(i,c)| if [8,13,18,23].contains(&i) {c==b'-'} else {c.is_ascii_hexdigit()}) ||
            !guid.bytes().any(|c| matches!(c,b'1'..=b'9'|b'a'..=b'f'|b'A'..=b'F')) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let now=Instant::now();
        let executable=system_wsl()?;
        let instance=guid.to_ascii_lowercase();
        let system_child=Command::new(&executable)
            .args(["--system","--user","root","--exec","/bin/sh","-c",
                include_str!("retire-system.sh"),"wsx-retire",&instance])
            .current_dir(executable.parent().unwrap()).stdin(Stdio::piped()).stdout(Stdio::piped())
            .stderr(Stdio::from(log.try_clone()?)).creation_flags(0x08000000).spawn()?;
        let normal_child=Command::new(&executable)
            .args(["--distribution",&distribution,"--user","root","--exec",
                "/usr/bin/python3","-c",include_str!("retire-guest.py"),&instance, if guest_protocol {"protocol-v1"} else {"legacy"}])
            .current_dir(executable.parent().unwrap()).stdin(Stdio::piped()).stdout(Stdio::piped())
            .stderr(Stdio::from(log.try_clone()?)).creation_flags(0x08000000).spawn()?;
        Ok(Self { executable,distribution,instance,log,
            child:None,result:None,attempted:false,bootstrap,vm_exited:false,system_retire:false,helper_failure_since:None,
            seen_running:false,distro_stopped:false,query:None,query_started:now,next_query:now,
            next_vm_query:now,system_child:Some(system_child),system_ready:false,system_started:now,
            normal_child:Some(normal_child),normal_ready:false,normal_started:now,thread:PhantomData })
    }

    fn check_vm(&mut self) -> io::Result<bool> {
        if self.vm_exited { return Ok(true); }
        if Instant::now()<self.next_vm_query {return Ok(false);}
        self.next_vm_query=Instant::now()+Duration::from_millis(100);
        match unsafe {wsx_direct_bootstrap_vm_exited(self.bootstrap)} {
            0 => {
                self.vm_exited=true;
                writeln!(self.log,"WSX_VM_EXIT_CONFIRMED instance={} source=owned-hcs-handle",self.instance)?;
                Ok(true)
            },
            1 => Ok(false),
            status => Err(io::Error::other(format!("Original VM exit observation failed: {status:#010x}"))),
        }
    }

    /// Poll bounded read-only WSL discovery without blocking queue service.
    /// Listing failure is uncertainty; socket EOF is never a retirement proof.
    pub fn poll_external(&mut self) -> io::Result<bool> {
        if self.check_vm()? {return Ok(true);}
        if self.distro_stopped {return Ok(true);}
        if let Some(query)=self.query.as_mut() {
            if let Some(status)=query.try_wait()? {
                if !status.success() {return Err(io::Error::other(format!("WSL lifetime query failed: {status}")));}
                let mut bytes=Vec::new();
                query.stdout.take().ok_or_else(||io::Error::other("Missing WSL list output"))?
                    .take(65537).read_to_end(&mut bytes)?;
                self.query.take();
                let running=policy::running_distribution(&bytes,&self.distribution).map_err(io::Error::other)?;
                if running { self.seen_running=true; }
                else if self.seen_running {
                    self.distro_stopped=true;
                    writeln!(self.log,"WSX_DISTRO_STOPPED instance={} distro={}",self.instance,self.distribution)?;
                    return Ok(true);
                }
                self.next_query=Instant::now()+Duration::from_secs(1);
            } else if self.query_started.elapsed()>Duration::from_secs(15) {
                return Err(io::Error::other("WSL lifetime query timed out; observation process retained"));
            }
        }
        if self.query.is_none() && Instant::now()>=self.next_query {
            self.query=Some(Command::new(&self.executable).args(["--list","--running","--quiet"])
                .current_dir(self.executable.parent().unwrap()).stdin(Stdio::null()).stdout(Stdio::piped())
                .stderr(Stdio::from(self.log.try_clone()?)).creation_flags(0x08000000).spawn()?);
            self.query_started=Instant::now();
        }
        Ok(false)
    }

    pub fn vm_exited(&self) -> bool { self.vm_exited }

    /// Bind both helpers to the running VM before desktop readiness. Normal
    /// close retains the verified session/systemd shutdown; external distro loss
    /// uses the system namespace. Neither path spawns wsl --exec during close.
    pub fn system_ready(&mut self) -> io::Result<bool> {
        for (child,ready,started,expected) in [
            (&mut self.system_child,&mut self.system_ready,self.system_started,b"WSX_SYSTEM_READY\n".as_slice()),
            (&mut self.normal_child,&mut self.normal_ready,self.normal_started,b"WSX_GUEST_READY\n".as_slice()),
        ] {
            if *ready {continue;}
            let process=child.as_mut().ok_or_else(||io::Error::other("Missing pre-bound retirement helper"))?;
            if let Some(status)=process.try_wait()? {return Err(io::Error::other(format!("Retirement helper exited before readiness: {status}")));}
            let stdout=process.stdout.as_mut().ok_or_else(||io::Error::other("Missing helper pipe"))?;
            let mut available=0;
            if unsafe {PeekNamedPipe(stdout.as_raw_handle(),std::ptr::null_mut(),0,std::ptr::null_mut(),&mut available,std::ptr::null_mut())}==0 {
                return Err(io::Error::last_os_error());
            }
            if available<expected.len() as u32 {
                if started.elapsed()>Duration::from_secs(15) {return Err(io::Error::other("Retirement helper readiness timed out"));}
                return Ok(false);
            }
            let mut bytes=vec![0;expected.len()];stdout.read_exact(&mut bytes)?;
            if bytes!=expected {return Err(io::Error::other("Invalid retirement helper handshake"));}
            *ready=true;
            writeln!(self.log,"WSX_RETIRE_HELPER_READY instance={} protocol={}",self.instance,String::from_utf8_lossy(expected).trim())?;
        }
        Ok(true)
    }

    /// Every close uses the helper bound before desktop readiness. Never spawn
    /// a Guest command here: checking VM/distro liveness cannot close that race.
    pub fn begin(&mut self) -> io::Result<()> {
        if self.check_vm()? {return Ok(());}
        if self.attempted {
            return if self.child.is_some() {Ok(())} else {Err(io::Error::other("Guest retirement request previously failed"))};
        }
        self.attempted=true;
        if !self.system_ready || !self.normal_ready {return Err(io::Error::other("Retirement helpers were not ready before desktop startup"));}
        self.system_retire=self.distro_stopped;
        let selected=if self.system_retire {&mut self.system_child} else {&mut self.normal_child};
        let mut child=selected.take().ok_or_else(||io::Error::other("Retirement helper already consumed"))?;
        let mut stdin=child.stdin.take().ok_or_else(||io::Error::other("Missing retirement command pipe"))?;
        // Retain the process before a fallible write. Broken pipe during VM loss
        // is not success; confirmed() must still observe the original HCS exit.
        self.child=Some(child);
        writeln!(self.log,"WSX_RETIRE_REQUEST instance={} source=prebound-{}",self.instance,if self.system_retire {"system-helper"} else {"guest-helper"})?;
        stdin.write_all(b"retire\n")?;
        Ok(())
    }

    /// A completed exact kernel retirement or confirmed original VM exit closes
    /// Guest admission. The Session still independently drains GPU/completions,
    /// every memory lease and HDV callbacks; errors never become success.
    pub fn confirmed(&mut self) -> io::Result<bool> {
        let vm_exited=self.check_vm()?;
        if self.result.is_none() {
            if let Some(child)=self.child.as_mut() {
                self.result=child.try_wait()?;
                if self.result.is_some() {
                    if let Some(stdout)=child.stdout.take() {
                        let mut bytes=Vec::new();stdout.take(65537).read_to_end(&mut bytes)?;
                        if bytes.len()>65536 {return Err(io::Error::other("System retirement output exceeds limit"));}
                        self.log.write_all(&bytes)?;
                    }
                }
            }
        }
        if vm_exited { return Ok(self.child.is_none() || self.result.is_some()); }
        match self.result {
            None => Ok(false),
            Some(status) if status.success() => Ok(true),
            Some(status) => {
                if !self.system_retire {
                    self.poll_external()?;
                    if self.vm_exited {return Ok(true);}
                    if self.distro_stopped {
                        self.child.take();self.result=None;self.attempted=false;self.helper_failure_since=None;
                        self.begin()?;return Ok(false);
                    }
                }
                // Helper exit can precede HCS VM-exit notification. Keep a
                // bounded reconciliation window; never launch a replacement.
                let since=self.helper_failure_since.get_or_insert_with(Instant::now);
                if since.elapsed()<Duration::from_secs(2) {Ok(false)}
                else {Err(io::Error::other(format!("Pre-bound Guest retirement failed: {status}")))}
            },
        }
    }

    pub fn started(&self) -> bool {self.child.is_some() || self.vm_exited}

    /// Close unused helper stdin and observe all helper processes without killing.
    /// This does not inspect bootstrap, which may already have been released.
    pub fn finish_helpers(&mut self) -> io::Result<()> {
        if let Some(child)=self.system_child.as_mut() {child.stdin.take();}
        if let Some(child)=self.normal_child.as_mut() {child.stdin.take();}
        let deadline=Instant::now()+Duration::from_secs(10);
        loop {
            let mut pending=false;
            for child in [&mut self.system_child,&mut self.normal_child,&mut self.query] {
                if let Some(process)=child.as_mut() {
                    if process.try_wait()?.is_some() {child.take();} else {pending=true;}
                }
            }
            if !pending {return Ok(());}
            if Instant::now()>=deadline {return Err(io::Error::other("Lifetime helpers did not exit; processes retained"));}
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for GuestRetirement {
    fn drop(&mut self) {
        // Dropping a Child never kills it. Keep unobserved process objects on
        // abandonment, matching the original adapter's failure policy.
        if self.result.is_none() {if let Some(child)=self.child.take() {std::mem::forget(child);}}
        if let Some(mut query)=self.query.take() {
            if !matches!(query.try_wait(),Ok(Some(_))) {std::mem::forget(query);}
        }
        for slot in [&mut self.system_child,&mut self.normal_child] {
            if let Some(mut helper)=slot.take() {
                helper.stdin.take();
                if !matches!(helper.try_wait(),Ok(Some(_))) {std::mem::forget(helper);}
            }
        }
    }
}
