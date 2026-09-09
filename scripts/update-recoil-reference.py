"""One-command CS2 recoil calibration. Requires Steam, HLAE, Python and Cargo on Windows."""
import argparse, csv, json, os, re, socket, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = json.loads((ROOT/'scripts/recoil-weapons.json').read_text(encoding='utf8'))

def select_weapons(tokens):
    selected=[]
    for token in tokens:
        for name in token.split(','):
            name=name.strip().lower()
            if name not in CATALOG: raise ValueError(f'Unknown weapon: {name!r}. Available: {", ".join(CATALOG)}')
            if name not in selected: selected.append(name)
    if not selected: raise ValueError('Select at least one weapon')
    return selected

def console(port, *commands):
    marker = b'DEMODESK_LAB_READY'
    with socket.create_connection(('127.0.0.1',port),timeout=3) as s:
        s.settimeout(1)
        s.sendall(('\n'.join(commands)+'\necho '+marker.decode()+'\n').encode())
        end = time.monotonic()+10; data = b''
        while marker not in data:
            if time.monotonic()>end: raise TimeoutError('CS2 console did not respond')
            try:
                chunk=s.recv(65536)
                if not chunk: raise ConnectionError('CS2 console closed')
                data+=chunk
            except socket.timeout: pass
        return data.decode(errors='replace')

def publish(candidate, destination):
    """Only replace the baseline after all recorded conditions agree within 0.01 degrees."""
    data=json.loads(candidate.read_text(encoding='utf8'))
    deviations=data['maxDeviationDegrees']
    if not deviations or any(not 0 <= value <= .01 for value in deviations.values()):
        raise ValueError('Trials disagree by more than 0.01 degrees; inspect evidence, baseline unchanged')
    for gun,points in data['weapons'].items():
        conditions={(e['stance'],e['nospread']) for e in data['evidence'] if e['weapon']==gun}
        if conditions != {('stand',False)}:
            raise ValueError(f'{gun}: missing posture/spread controls')
        if any(p['samples']<1 for p in points): raise ValueError(f'{gun}: no recorded samples')
    if destination.exists():
        previous=json.loads(destination.read_text(encoding='utf8'))
        unrecorded=set(previous['weapons'])-set(data['weapons'])
        if unrecorded:
            if previous['patchVersion']!=data['patchVersion']:
                raise ValueError('New game build: record all existing weapons or use a separate --output')
            data['weapons']={**previous['weapons'],**data['weapons']}
            data['maxDeviationDegrees']={**previous['maxDeviationDegrees'],**deviations}
            data['evidence']=[e for e in previous['evidence'] if e['weapon'] in unrecorded]+data['evidence']
    destination.parent.mkdir(parents=True,exist_ok=True)
    temporary=destination.with_suffix('.json.tmp')
    temporary.write_text(json.dumps(data,indent=2)+'\n',encoding='utf8')
    temporary.replace(destination)

def main():
    cli=argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--game',type=Path,required=True,help='CS2 game/csgo directory')
    cli.add_argument('--hlae',type=Path,default=ROOT/'target/debug/demodesk-data/tools/hlae',help='HLAE directory')
    cli.add_argument('--port',type=int,default=21347)
    cli.add_argument('--weapons',nargs='+',default=list(CATALOG),help='Comma or space separated names, e.g. m4a1_silencer,ak47,m4a1')
    cli.add_argument('--output',type=Path,default=ROOT/'src/data/recoil-reference.json')
    args=cli.parse_args()
    try: args.weapons=select_weapons(args.weapons)
    except ValueError as error: cli.error(str(error))
    if sys.flags.optimize: cli.error('Do not use python -O: validation assertions must be enabled')
    if os.name!='nt': cli.error('This capture launcher requires Windows')
    args.game=args.game.resolve(); args.hlae=args.hlae.resolve()
    game=args.game.parent/'bin/win64/cs2.exe'; launcher=args.hlae/'HLAE.exe'; hook=args.hlae/'x64/AfxHookSource2.dll'
    for path in (game,launcher,hook):
        if not path.is_file(): cli.error(f'Not found: {path}')
    processes=subprocess.check_output(['tasklist','/FI','IMAGENAME eq cs2.exe','/FO','CSV'],text=True)
    if 'cs2.exe' in processes.lower(): cli.error('Close CS2 first; calibration launches its own isolated game')
    os.chdir(ROOT)
    work=ROOT/'target/recoil-lab'/time.strftime('update_%Y%m%d_%H%M%S')
    work.mkdir(parents=True); (work/'cfg').mkdir()
    lock=ROOT/'target/recoil-lab/update.lock'
    try: lock.open('x').close()
    except FileExistsError: cli.error(f'Another calibration may be running. Check before removing {lock}')
    started=False; game_pid=None
    try:
        print('Building shot exporter...',flush=True)
        subprocess.run(['cargo','build','-p','demodesk-core','--example','recoil_lab','--target-dir','target/codex-review'],check=True)
        window_hook=work/'demodesk-window-hook.dll'
        (work/'Detours.LICENSE.md').write_bytes((ROOT/'crates/demodesk-core/window-hook/Detours.LICENSE.md').read_bytes())
        subprocess.run(['target/codex-review/debug/examples/recoil_lab.exe','--window-hook',str(window_hook)],check=True)
        command=f'-insecure -novid -windowed -width 1280 -height 720 -netconport {args.port} -afxFixNetCon -afxDisableSteamStorage +engine_no_focus_sleep 0 +volume 0 +unbindall +m_yaw 0 +m_pitch 0'
        startup=subprocess.STARTUPINFO(); startup.dwFlags|=subprocess.STARTF_USESHOWWINDOW; startup.wShowWindow=0
        subprocess.Popen([str(launcher),'-noGui','-autoStart','-noConfig','-afxDisableSteamStorage','-customLoader','-hookDllPath',str(window_hook),'-hookDllPath',str(hook),'-programPath',str(game),'-cmdLine',command],env={**os.environ,'USRLOCALCSGO':str(work/'cfg'),'DEMODESK_WINDOW_HOOK_LOG':str(work/'window-hook.log')},startupinfo=startup)
        started=True
        deadline=time.monotonic()+120
        while True:
            rows=csv.reader(subprocess.check_output(['tasklist','/FI','IMAGENAME eq cs2.exe','/FO','CSV','/NH'],text=True).splitlines())
            game_pid=next((int(row[1]) for row in rows if row[0].lower()=='cs2.exe'),None)
            try: console(args.port,'tv_enable 1','map de_dust2'); break
            except (OSError,TimeoutError):
                if time.monotonic()>deadline: raise TimeoutError('CS2/HLAE startup timed out; check Steam and HLAE')
                print('Waiting for CS2...',flush=True); time.sleep(3)
        if 'installed' not in (work/'window-hook.log').read_text(encoding='utf8'):
            raise RuntimeError('Window isolation hook did not install; refusing to record')
        print('Loading local map...',flush=True); time.sleep(15)
        console(args.port,'sv_cheats 1','mp_limitteams 0','mp_autoteambalance 0','mp_freezetime 0',
                'mp_ignore_round_win_conditions 1','mp_roundtime 60','mp_roundtime_defuse 60',
                'mp_roundtime_hostage 60','host_timescale 1','sv_infinite_ammo 2','mp_respawn_on_death_t 1','mp_respawn_on_death_ct 1',
                'volume 0','unbindall','m_yaw 0','m_pitch 0','bot_kick','jointeam 2','joinclass 1','mp_restartgame 1')
        time.sleep(8)
        isolation=console(args.port,'volume','m_yaw','m_pitch','key_listboundkeys')
        (work/'input-isolation.log').write_text(isolation,encoding='utf8')
        values=dict(re.findall(r'(?m)^(volume|m_yaw|m_pitch)\s*=\s*(\S+)',isolation))
        if 'Unknown command' in isolation or any(float(values.get(setting,'nan'))!=0 for setting in ('volume','m_yaw','m_pitch')):
            raise RuntimeError('Could not verify muted, unbound mouse input')
        print('Recording standing, once per weapon (hidden, muted, unbound input)...',flush=True)
        subprocess.run([sys.executable,'-u','scripts/capture-recoil-lab.py','--game',str(args.game),
            '--out',str(work),'--port',str(args.port),'--repeats','1','--batches','1',
            '--normal-spread','--stances','stand','--weapons',*args.weapons],check=True)
        manifests=list(work.glob('*-manifest.json'))
        if len(manifests)!=1: raise RuntimeError('Expected one completed recording manifest')
        subprocess.run([sys.executable,'scripts/analyze-recoil-lab.py',str(manifests[0]),
            '--output',str(work/'candidate.json')],check=True)
        publish(work/'candidate.json',args.output)
        print(f'Updated {args.output}\nEvidence: {work}',flush=True)
    finally:
        if started:
            try: console(args.port,'-attack','-reload','-duck','tv_stoprecord','weapon_accuracy_nospread false','quit')
            except (OSError,TimeoutError): pass
            if game_pid:
                for _ in range(10):
                    status=subprocess.check_output(['tasklist','/FI',f'PID eq {game_pid}','/FO','CSV','/NH'],text=True)
                    if 'cs2.exe' not in status.lower(): break
                    time.sleep(1)
                else:
                    subprocess.run(['taskkill','/PID',str(game_pid),'/F'],check=True,stdout=subprocess.DEVNULL)
                print('Calibration game closed.',flush=True)
        lock.unlink(missing_ok=True)

if __name__=='__main__': main()
