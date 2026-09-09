"""Small regression check for calibration acceptance, without needing CS2."""
import copy, importlib.util
from pathlib import Path
spec = importlib.util.spec_from_file_location('lab', Path(__file__).with_name('analyze-recoil-lab.py'))
lab = importlib.util.module_from_spec(spec); spec.loader.exec_module(lab)
trial = {'weapon':'ak47','stance':'stand','file':'synthetic'}
shots = [dict(event_name='fire_bullets', tick=i*6, item_def_index=7, mode=0, recoil_index=i,
    num_bullets_remaining=30-i, user_is_alive=True, user_steamid='p', user_ducked=False,
    user_duck_amount=0, user_X=0,user_Y=0,user_Z=0,user_pitch=0,user_yaw=0,
    angles_x=-float(i),angles_y=float(i)) for i in range(30)]
assert len(lab.validate(shots,trial,1)[0]) == 30
for key,value in [('item_def_index',16),('recoil_index',9),('user_X',1),('user_pitch',1),
                  ('user_duck_amount',.5),('user_is_alive',False),('angles_y',float('nan'))]:
    bad=copy.deepcopy(shots);bad[5][key]=value
    try: lab.validate(bad,trial,1)
    except AssertionError: pass
    else: raise AssertionError(f'Accepted invalid {key}')
spec = importlib.util.spec_from_file_location('update', Path(__file__).with_name('update-recoil-reference.py'))
update = importlib.util.module_from_spec(spec); spec.loader.exec_module(update)
assert update.select_weapons(['m4a1_silencer,ak47','m4a1','ak47']) == ['m4a1_silencer','ak47','m4a1']
try: update.select_weapons(['m4a4'])
except ValueError: pass
else: raise AssertionError('Only internal weapon names are accepted')
import json, tempfile
with tempfile.TemporaryDirectory() as folder:
    candidate=Path(folder)/'candidate.json'; destination=Path(folder)/'baseline.json'
    data={'patchVersion':1,'weapons':{'ak47':[{'x':0,'y':0,'samples':1}]},
        'maxDeviationDegrees':{'ak47':0},'evidence':[{'weapon':'ak47','stance':s,'nospread':False} for s in ('stand',)]}
    candidate.write_text(json.dumps(data)); update.publish(candidate,destination)
    good=destination.read_bytes()
    for deviation in (.02,float('nan')):
        bad=copy.deepcopy(data);bad['maxDeviationDegrees']['ak47']=deviation
        candidate.write_text(json.dumps(bad))
        try: update.publish(candidate,destination)
        except ValueError: pass
        else: raise AssertionError('Invalid calibration replaced baseline')
        assert destination.read_bytes()==good
    partial=copy.deepcopy(data);partial['weapons']={'m4a1':partial['weapons'].pop('ak47')}
    partial['maxDeviationDegrees']={'m4a1':0}
    for e in partial['evidence']:e['weapon']='m4a1'
    candidate.write_text(json.dumps(partial));update.publish(candidate,destination)
    assert set(json.loads(destination.read_text())['weapons'])=={'ak47','m4a1'}
    good=destination.read_bytes();partial['patchVersion']=2;candidate.write_text(json.dumps(partial))
    try:update.publish(candidate,destination)
    except ValueError:pass
    else:raise AssertionError('Mixed game versions')
    assert destination.read_bytes()==good
print('Calibration checks passed: invalid samples, weapon parameters, atomic update, partial merge and build isolation')
