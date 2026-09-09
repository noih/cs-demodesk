"""Capture controlled offline CS2/GOTV recoil trials through an existing netcon session."""
import argparse, json, shutil, socket, time
from pathlib import Path

catalog = json.loads(Path(__file__).with_name('recoil-weapons.json').read_text(encoding='utf8'))
parser = argparse.ArgumentParser()
parser.add_argument('--weapons', nargs='+', choices=list(catalog), default=list(catalog))
parser.add_argument('--game', type=Path, required=True, help='CS2 game/csgo directory')
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--port', type=int, default=21347)
parser.add_argument('--repeats', type=int, default=1)
parser.add_argument('--batches', type=int, default=1, choices=(1, 2))
parser.add_argument('--normal-spread', action='store_true')
parser.add_argument('--stances', nargs='+', choices=('stand','crouch'), default=['stand','crouch'])
args = parser.parse_args()
if not 1 <= args.repeats <= 30: parser.error('repeats must be between 1 and 30')
args.out.mkdir(parents=True, exist_ok=True)
run = time.strftime('%Y%m%d_%H%M%S')
weapons = [(w, catalog[w]['magazine'], catalog[w]['fireSeconds']) for w in args.weapons]
manifest = {'run': run, 'repeats': args.repeats, 'batches': args.batches, 'nospread': not args.normal_spread, 'recoverySeconds': 4, 'weapons': args.weapons, 'stances': args.stances, 'trials': []}
(args.out/f'{run}-manifest.json').write_text(json.dumps(manifest,indent=2),encoding='utf8')
log = (args.out / f'{run}-console.log').open('w', encoding='utf8')
s = socket.create_connection(('127.0.0.1', args.port), timeout=3)
s.settimeout(.05)
def send(*commands):
    s.sendall(('\n'.join(commands)+'\n').encode())
    log.write('> '+'; '.join(commands)+'\n'); log.flush()
def pause(seconds):
    end = time.monotonic()+seconds
    while time.monotonic()<end:
        try:
            data=s.recv(65536)
            if not data: raise RuntimeError('CS2 netcon closed')
            log.write(data.decode(errors='replace')); log.flush()
        except socket.timeout: pass
try:
    send('sv_cheats 1','bot_kick','mp_ignore_round_win_conditions 1','mp_roundtime 60','mp_roundtime_defuse 60','mp_roundtime_hostage 60','host_timescale 1',
         'sv_infinite_ammo 2','tv_delay 0','tv_record_immediate 1',f'weapon_accuracy_nospread {str(not args.normal_spread).lower()}')
    for batch in range(1, args.batches+1):
        for weapon,magazine,seconds in weapons:
            for stance in args.stances:
                crouched=stance=='crouch'
                name=f'demodesk_recoil_{run}_{weapon}_{stance}_{batch}'
                send('-attack','-reload', *(f'ent_fire weapon_{w} kill' for w in catalog))
                pause(1)
                send(f'give weapon_{weapon}')
                pause(1)
                send('slot1',
                     '+duck' if crouched else '-duck','setang 0 0 0')
                pause(4)
                send(f'tv_record {name}')
                for repeat in range(args.repeats):
                    send('-attack','+reload'); pause(4)
                    send('-reload','setang 0 0 0','+attack'); pause(seconds)
                    send('-attack')
                    print(f'{weapon} {stance} batch {batch}: {repeat+1}/{args.repeats}',flush=True)
                send('tv_stoprecord'); pause(1)
                source=args.game/f'{name}.dem'
                destination=args.out/source.name
                shutil.copy2(source,destination)
                manifest['trials'].append({'weapon':weapon,'stance':stance,'batch':batch,'magazine':magazine,'file':destination.name})
                (args.out/f'{run}-manifest.json').write_text(json.dumps(manifest,indent=2),encoding='utf8')
finally:
    try: send('-attack','-reload','-duck','tv_stoprecord','weapon_accuracy_nospread false')
    finally: s.close(); log.close()
