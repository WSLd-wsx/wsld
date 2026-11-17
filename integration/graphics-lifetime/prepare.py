"""Apply external-lifetime handling only to a checked, staged close-repair tree."""
from pathlib import Path

HERE = Path(__file__).resolve().parent


def apply(native, edit, replace):
    def plasma_retirement(source):
        helper = (HERE/'plasma-retirement.py').read_text() + '\n' + (HERE/'protocol-retirement.py').read_text()
        source = replace(source, '\ndef main():', '\n' + helper + '\n\ndef main():')
        start = source.index("    unit = f'wsx-omarchy-{instance.hex}.service'")
        end = source.index('    # The existing owner retires', start)
        old = source[start:end]
        return source[:start] + '    if len(sys.argv) > 2 and sys.argv[2] == \"protocol-v1\":\n        stop_protocol(instance)\n    elif not stop_plasma(instance):\n' + ''.join('    ' + line if line.strip() else line for line in old.splitlines(True)) + source[end:]
    edit(native/'retire-guest.py', plasma_retirement)
    edit(native/'retire-guest.py', lambda s: replace(s,
        "if __name__ == '__main__':\n    main()",
        "if __name__ == '__main__':\n"
        "    # Bind this exact distro/VM before rendering, never at close time.\n"
        "    print('WSX_GUEST_READY', flush=True)\n"
        "    command = sys.stdin.readline()\n"
        "    if command == '':\n        sys.exit(0)\n"
        "    if command != 'retire\\n':\n        raise ValueError('Invalid retirement command')\n"
        "    main()"))
    (native/'aperture-exit.h').write_bytes((HERE/'aperture-exit.h').read_bytes())
    (native/'native-call-trace.h').write_bytes((HERE/'native-call-trace.h').read_bytes())
    edit(native/'host.h', lambda s: replace(s, '    HANDLE thread_reference;',
        '    HANDLE thread_reference;\n'
        '    /* Borrowed exact HCS generation; direct bootstrap outlives host. */\n'
        '    HANDLE original_system;'))
    edit(native/'host.c', lambda s: replace(s, '    h->proxy = proxy;',
        '    h->original_system = system;\n    h->proxy = proxy;'))
    edit(native/'guest-memory.c', lambda s: replace(replace(s,
        '#include "guest-memory.h"', '#include "guest-memory.h"\n#include "aperture-exit.h"'),
        '        status = HdvDestroyGuestMemoryAperture(m->device,m->address);\n        if (status != S_OK) return status;',
        '        status = HdvDestroyGuestMemoryAperture(m->device,m->address);\n'
        '        if (status != S_OK) {\n'
        '            fprintf(stderr,"WSX_GUEST_RAM_NATIVE: destroy_failed gpa=0x%llx address=%p bytes=%lu hr=0x%08lx\\n",\n'
        '                (unsigned long long)m->guest_address,m->address,(unsigned long)m->size,(unsigned long)(uint32_t)status);\n'
        '            status=wsx_aperture_reconcile_exit(m->host->original_system,m->address,m->size,status);\n'
        '            if(status!=S_OK)return status;\n'
        '        }'))
    for name in ('guest_retirement.rs', 'guest_retirement_tests.rs', 'lifetime_policy.rs', 'retire-system.sh'):
        target = native/name
        if target.exists():
            edit(target, lambda s, name=name: (HERE/name).read_text())
        else:
            target.write_bytes((HERE/name).read_bytes())
    edit(native/'direct-owner.h', lambda s: replace(s,
        '    BOOL removing;', '    BOOL removing, vm_exited;').replace(
        '#endif', 'HRESULT wsx_direct_vm_exited(struct wsx_direct_owner *);\n'
        'HRESULT wsx_direct_revoke_exited_vm(struct wsx_direct_owner *);\n#endif'))
    edit(native/'direct-owner.c', lambda s: s + r'''

/* This queries the retained handle of the original VM, never a new VM found
 * by name or process absence. Timeout and every unknown error are not proof. */
HRESULT wsx_direct_vm_exited(struct wsx_direct_owner *o)
{
    HRESULT hr=check(o);if(hr!=S_OK)return hr;
    if(o->vm_exited)return S_OK;
    if(!o->system||!o->module)return E_UNEXPECTED;
    FARPROC raw=GetProcAddress(o->module,"HcsWaitForComputeSystemExit");
    if(!raw)return E_NOTIMPL;
    Wait wait;_Static_assert(sizeof(wait)==sizeof(raw),"HCS exit ABI");
    memcpy(&wait,&raw,sizeof(wait));
    PWSTR result=NULL;hr=wait(o->system,0,&result);
    if(result)LocalFree(result);
    if(hr==S_OK){o->vm_exited=TRUE;return S_OK;}
    if(hr==HRESULT_FROM_WIN32(WAIT_TIMEOUT)||hr==HRESULT_FROM_WIN32(ERROR_TIMEOUT)||
       hr==(HRESULT)0x80370117L||hr==(HRESULT)0x80370118L)return S_FALSE;
    return hr;
}

/* Only actual VM exit can substitute for removing a device from that VM.
 * Native/GPU/mapping drains and HDV teardown remain separately mandatory. */
HRESULT wsx_direct_revoke_exited_vm(struct wsx_direct_owner *o)
{
    HRESULT hr=wsx_direct_vm_exited(o);if(hr!=S_OK)return hr;
    if(o->operation){
        PWSTR result=NULL;
        hr=resolve_HcsWaitForOperationResult(o->module)(o->operation,0,&result);
        if(result)LocalFree(result);
        if(hr==HRESULT_FROM_WIN32(WAIT_TIMEOUT)||hr==HRESULT_FROM_WIN32(ERROR_TIMEOUT)||
           hr==(HRESULT)0x80370117L||hr==(HRESULT)0x80370118L)return S_FALSE;
        /* The operation has completed, possibly with VM-already-exited. Its
         * failure is not promoted to success: the retained VM exit is authority. */
        resolve_HcsCloseOperation(o->module)(o->operation);o->operation=NULL;
    }
    o->phase=WSX_DIRECT_REMOVED;
    return S_OK;
}
''')
    edit(native/'direct-bootstrap.c', lambda s: replace(s,
        '    if(!s->removal_started){',
        '    HRESULT exited=wsx_direct_vm_exited(&s->owner);\n'
        '    if(exited==S_OK){\n'
        '        HRESULT hr=wsx_direct_revoke_exited_vm(&s->owner);if(hr!=S_OK)return hr;\n'
        '        return SetEvent(s->revoked)?S_OK:HRESULT_FROM_WIN32(GetLastError());\n'
        '    }\n    if(exited!=S_FALSE)return exited;\n'
        '    if(!s->removal_started){') + r'''

HRESULT wsx_direct_bootstrap_vm_exited(struct wsx_child_bootstrap *s)
{
    if(!s||!s->constructed)return E_UNEXPECTED;
    return wsx_direct_vm_exited(&s->owner);
}
''')
    edit(native/'runtime.rs', lambda s: replace(s,
        '    let requested = phase(runtime)? >= STOPPED ||',
        '    if let Some(guest)=guest {\n'
        '        let mut g=guest.try_borrow_mut().map_err(|_|io::Error::other("Guest controller is borrowed"))?;\n'
        '        if g.poll_external()? { g.begin()?; }\n'
        '        if g.vm_exited() { return Ok(true); }\n'
        '    }\n'
        '    let requested = phase(runtime)? >= STOPPED ||'))
    edit(native/'native_main.rs', lambda s: replace(replace(s,
        'profile.distribution.clone(), launch.instance.text(), log.try_clone().map_err(debug)?)',
        'profile.distribution.clone(), launch.instance.text(), log.try_clone().map_err(debug)?, state.bootstrap)'),
        '        report("failed stream quarantined before Guest device retirement");\n        return Err(debug(error));',
        '        // A real VM exit can race a socket or HDV error. Observe the\n'
        '        // original HCS handle; never recover merely from EOF.\n'
        '        let deadline=std::time::Instant::now()+Duration::from_secs(2);\n'
        '        loop {\n'
        '            let mut guest=state.guest.as_ref().unwrap().borrow_mut();\n'
        '            let _=guest.poll_external();\n'
        '            if guest.vm_exited() { report("VM exited; entering native drain after stream loss"); break; }\n'
        '            if std::time::Instant::now()>=deadline {\n'
        '                report("failed stream quarantined before Guest device retirement");\n'
        '                return Err(debug(error));\n'
        '            }\n'
        '            drop(guest);std::thread::sleep(Duration::from_millis(100));\n'
        '        }'))
    edit(native/'native_main.rs', lambda s: replace(replace(replace(s,
        '    report("native ownership adopted; initializing Venus backend");',
        '    loop {\n'
        '        let mut guest=state.guest.as_ref().unwrap().borrow_mut();\n'
        '        if guest.system_ready().map_err(debug)? { break; }\n'
        '        guest.poll_external().map_err(debug)?;\n'
        '        if guest.vm_exited() { return Err("VM exited before system helper readiness".into()); }\n'
        '        drop(guest);std::thread::sleep(Duration::from_millis(20));\n'
        '    }\n'
        '    report("native ownership adopted; initializing Venus backend");'),
        '            Err(error) => return Err(debug(error)),\n        }\n        std::thread::sleep(Duration::from_millis(100));',
        '            Err(runtime::Error::Memory(wsx_hdv_memory_session::Error::Completion(\n'
        '                wsx_standard_gpu::completion::CompletionError::Pending(_)))) => (),\n'
        '            Err(runtime::Error::Memory(wsx_hdv_memory_session::Error::Mapping(\n'
        '                wsx_standard_gpu::mapping::Error::Backend(rutabaga_gfx::RutabagaError::ComponentError(1))))) => (),\n'
        '            Err(error) => return Err(debug(error)),\n        }\n        std::thread::sleep(Duration::from_millis(100));'),
        '    Ok(())\n}\n\nfn main()',
        '    state.guest.as_ref().unwrap().borrow_mut().finish_helpers().map_err(debug)?;\n'
        '    report("lifetime helpers exited without forced termination");\n'
        '    Ok(())\n}\n\nfn main()'))
    # Diagnostic begin/end records locate synchronous API stalls without moving
    # calls off the owning COM thread or changing any ownership/failure policy.
    edit(native/'guest-memory.c', lambda s: replace(replace(replace(s,
        '#include "aperture-exit.h"', '#include "aperture-exit.h"\n#include "native-call-trace.h"'),
        '    status = HdvCreateGuestMemoryAperture(device,gpa,(UINT32)size,!writable,&m->address);',
        '    struct wsx_call_trace trace=wsx_call_begin("HdvCreateGuestMemoryAperture",gpa);\n'
        '    status = HdvCreateGuestMemoryAperture(device,gpa,(UINT32)size,!writable,&m->address);\n'
        '    wsx_call_end(trace,status);'),
        '        status = HdvDestroyGuestMemoryAperture(m->device,m->address);',
        '        struct wsx_call_trace trace=wsx_call_begin("HdvDestroyGuestMemoryAperture",m->guest_address);\n'
        '        status = HdvDestroyGuestMemoryAperture(m->device,m->address);\n'
        '        wsx_call_end(trace,status);'))
    edit(native/'direct-owner.c', lambda s: replace(replace(s,
        '#include "direct-owner.h"', '#include "direct-owner.h"\n#include "native-call-trace.h"'),
        '    PWSTR result=NULL;hr=wait(o->system,0,&result);',
        '    struct wsx_call_trace trace=wsx_call_begin("HcsWaitForComputeSystemExit",0);\n'
        '    PWSTR result=NULL;hr=wait(o->system,0,&result);\n'
        '    wsx_call_end(trace,hr);'))

    edit(native/'native_main.rs', lambda s: replace(replace(s,
        'struct Profile {', 'struct Profile {\n    #[serde(default)] guest_protocol: bool,'),
        'log.try_clone().map_err(debug)?, state.bootstrap)',
        'log.try_clone().map_err(debug)?, state.bootstrap, profile.guest_protocol)'))
