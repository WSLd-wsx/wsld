"""Authorize only DRM siblings belonging to an admitted rootfs instance."""
import argparse,json,os,pwd,stat,subprocess,uuid
from pathlib import Path

def trusted_json(path):
    st=path.lstat()
    if not stat.S_ISREG(st.st_mode) or st.st_uid!=0 or st.st_mode & 0o022:
        raise RuntimeError('Untrusted Guest descriptor')
    return json.loads(path.read_text())

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--instance',required=True)
    args=parser.parse_args();instance=uuid.UUID(args.instance)
    if os.geteuid()!=0 or not instance.int:raise RuntimeError('Root and nonzero instance required')
    spec=trusted_json(Path('/run/wsx')/(instance.hex+'.json'))
    initialized=trusted_json(Path('/var/lib/wsx/initialized.json'))
    if spec['instance_id']!=str(instance) or spec['user']!=initialized['user']:
        raise RuntimeError('Initialization/instance identity mismatch')
    account=pwd.getpwnam(initialized['user'])
    if account.pw_uid==0:raise RuntimeError('Root desktop is forbidden')
    card=Path(spec['drm_node'])
    if card.parent!=Path('/dev/dri') or not card.name.startswith('card') or not card.name[4:].isdigit():
        raise RuntimeError('Invalid DRM card')
    device=(Path('/sys/class/drm')/card.name/'device').resolve(strict=True)
    ancestors=[p for p in device.parents if (p/'subsystem').is_symlink() and (p/'subsystem').resolve().name=='vmbus']
    if len(ancestors)!=1 or uuid.UUID((ancestors[0]/'device_id').read_text().strip().strip('{}'))!=instance:
        raise RuntimeError('DRM device does not belong to the admitted instance')
    nodes=[]
    for entry in (device/'drm').iterdir():
        if not ((entry.name.startswith('card') and entry.name[4:].isdigit()) or
                (entry.name.startswith('renderD') and entry.name[7:].isdigit())):continue
        node=Path('/dev/dri')/entry.name;st=node.lstat()
        expected=tuple(map(int,(entry/'dev').read_text().strip().split(':')))
        if not stat.S_ISCHR(st.st_mode) or (os.major(st.st_rdev),os.minor(st.st_rdev))!=expected:
            raise RuntimeError('Device node identity mismatch')
        if (entry/'device').resolve(strict=True)!=device:raise RuntimeError('Foreign DRM sibling')
        nodes.append(node)
    if card not in nodes or len(nodes)!=2 or not any(n.name.startswith('renderD') for n in nodes):
        raise RuntimeError('Expected one exact card and render sibling')
    subprocess.run(['setfacl','-m',f'u:{account.pw_uid}:rw',*[str(n) for n in nodes]],check=True)
    subprocess.run(['runuser','-u',account.pw_name,'--','python3','-c',
                    'import os,sys; [os.close(os.open(p,os.O_RDWR|os.O_CLOEXEC)) for p in sys.argv[1:]]',
                    *[str(n) for n in nodes]],check=True)
    print(json.dumps({'instance':str(instance),'user':account.pw_name,'uid':account.pw_uid,
                      'nodes':[str(n) for n in nodes],'user_open_verified':True}))
if __name__=='__main__':main()
