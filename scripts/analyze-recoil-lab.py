"""Validate controlled shot captures and export reproducible angular references (stdlib only)."""
import argparse, hashlib, json, math, statistics, subprocess
from pathlib import Path

GUNS = json.loads(Path(__file__).with_name('recoil-weapons.json').read_text(encoding='utf8'))

def validate(events, trial, repeats):
    shots = sorted((e for e in events if e['event_name'] == 'fire_bullets'), key=lambda e: e['tick'])
    config = GUNS[trial['weapon']]
    item, size = config['itemId'], config['magazine']
    assert len(shots) == size * repeats, (trial['file'], 'shot count', len(shots))
    bursts = []
    for offset in range(0, len(shots), size):
        burst = shots[offset:offset+size]
        first = burst[0]
        for i, e in enumerate(burst):
            assert e['item_def_index'] == item and e['mode'] == config['mode']
            assert e['recoil_index'] == i and e['num_bullets_remaining'] == size-i
            assert e['user_is_alive'] and e['user_steamid'] == first['user_steamid']
            crouch = trial['stance'] == 'crouch'
            assert e['user_ducked'] == crouch and abs(e['user_duck_amount']-int(crouch)) < .001
            assert all(abs(e[k]-first[k]) < .001 for k in ('user_X','user_Y','user_Z','user_pitch','user_yaw'))
            assert abs(e['user_pitch']) < .001 and abs(e['user_yaw']) < .001
            assert all(math.isfinite(e[k]) for k in ('angles_x','angles_y'))
            if i: assert config['tickGap'][0] <= e['tick']-burst[i-1]['tick'] <= config['tickGap'][1]
        assert abs(first['angles_x']) < .001 and abs(first['angles_y']) < .001, 'Unrecovered first shot'
        if offset: assert first['tick']-shots[offset-1]['tick'] >= 4*64
        bursts.append([[e['angles_y'], e['angles_x']] for e in burst])
    return bursts

def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('manifests', type=Path, nargs='+')
    cli.add_argument('--exporter', type=Path, default=Path('target/codex-review/debug/examples/recoil_lab.exe'))
    cli.add_argument('--output', type=Path, required=True)
    args = cli.parse_args()
    samples = {}
    evidence, patches = [], set()
    for manifest_path in args.manifests:
        manifest = json.loads(manifest_path.read_text(encoding='utf8'))
        selected = manifest.get('weapons',list(dict.fromkeys(t['weapon'] for t in manifest['trials'])))
        expected = {(gun, stance, batch) for gun in selected for stance in manifest.get('stances',['stand','crouch']) for batch in range(1,manifest['batches']+1)}
        assert {(t['weapon'],t['stance'],t['batch']) for t in manifest['trials']} == expected, 'Incomplete recording batch'
        for t in manifest['trials']:
            demo = manifest_path.parent/t['file']; output = demo.with_suffix('.json')
            subprocess.run([str(args.exporter),str(demo),str(output)],check=True)
            data = json.loads(output.read_text(encoding='utf8'))
            patches.add(data['header']['patch_version'])
            bursts = validate(data['events'],t,manifest['repeats'])
            samples.setdefault(t['weapon'],[]).extend(bursts)
            evidence.append({**t,'nospread':manifest['nospread'],'bursts':len(bursts),'sha256':hashlib.sha256(demo.read_bytes()).hexdigest()})
    assert len(patches) == 1, 'Do not pool different game versions'
    weapons, deviations = {}, {}
    for gun, bursts in samples.items():
        points = [[statistics.mean(b[i][axis] for b in bursts) for axis in (0,1)] for i in range(GUNS[gun]['magazine'])]
        deviations[gun] = max(abs(b[i][axis]-points[i][axis]) for b in bursts for i in range(len(points)) for axis in (0,1))
        weapons[gun] = [{'x':x,'y':y,'samples':len(bursts)} for x,y in points]
    result = {'patchVersion':int(next(iter(patches))),'conditions':'Stationary; fixed eye angles; fully recovered; full-auto magazine; posture and spread settings recorded in evidence. Firing direction, not bullet impacts.','maxDeviationDegrees':deviations,'weapons':weapons,'evidence':evidence}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
    print(json.dumps({'patchVersion':result['patchVersion'],'bursts':{g:len(b) for g,b in samples.items()},'maxDeviationDegrees':deviations},indent=2))

if __name__ == '__main__':
    if not __debug__: raise RuntimeError('Do not use python -O: sample validation must be enabled')
    main()
