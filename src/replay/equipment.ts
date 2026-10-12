// Parser display names and kill-event identifiers share the same local icon set.
const EQUIPMENT: Record<string, string> = {
  'Desert Eagle': 'deagle', 'Dual Berettas': 'elite', 'Five-SeveN': 'fiveseven', 'Glock-18': 'glock',
  'AK-47': 'ak47', AUG: 'aug', AWP: 'awp', FAMAS: 'famas', G3SG1: 'g3sg1', 'Galil AR': 'galilar',
  M249: 'm249', M4A4: 'm4a1', 'MAC-10': 'mac10', P90: 'p90', 'MP5-SD': 'mp5sd', 'UMP-45': 'ump45',
  XM1014: 'xm1014', 'PP-Bizon': 'bizon', 'MAG-7': 'mag7', Negev: 'negev', 'Sawed-Off': 'sawedoff',
  'Tec-9': 'tec9', 'Zeus x27': 'taser', P2000: 'hkp2000', MP7: 'mp7', MP9: 'mp9', Nova: 'nova',
  P250: 'p250', 'SCAR-20': 'scar20', 'SG 553': 'sg556', 'SSG 08': 'ssg08', 'M4A1-S': 'm4a1_silencer',
  'USP-S': 'usp_silencer', 'CZ75-Auto': 'cz75a', 'R8 Revolver': 'revolver',
  Flashbang: 'flashbang', 'High Explosive Grenade': 'hegrenade', 'Smoke Grenade': 'smokegrenade',
  Molotov: 'molotov', 'Decoy Grenade': 'decoy', 'Incendiary Grenade': 'incgrenade',
  'C4 Explosive': 'c4', 'Defuse Kit': 'defuser', 'Medi-Shot': 'healthshot', Knife: 'knife',
  inferno: 'inferno',
};
const ICONS = new Set(Object.values(EQUIPMENT));

export function equipmentId(name: string): string | undefined {
  const id = EQUIPMENT[name] ?? name.toLowerCase().replace(/^weapon_/, '');
  if (/knife|bayonet|karambit|daggers/i.test(id)) return 'knife';
  if (id === 'm4a1_silencer_off') return 'm4a1_silencer';
  if (id === 'usp_silencer_off') return 'usp_silencer';
  return ICONS.has(id) ? id : undefined;
}

export function equipmentIcon(name: string): string | undefined {
  const id = equipmentId(name);
  return id ? `/weapons/${id}.svg` : undefined;
}

const UTILITY = new Set(['flashbang', 'hegrenade', 'smokegrenade', 'molotov', 'decoy', 'incgrenade', 'c4', 'defuser', 'healthshot']);
export function isUtility(name: string): boolean {
  return UTILITY.has(equipmentId(name) ?? '');
}

const images = new Map<string, HTMLImageElement>();

const PISTOLS = new Set(['deagle', 'elite', 'fiveseven', 'glock', 'hkp2000', 'p250', 'tec9', 'usp_silencer', 'cz75a', 'revolver']);
const GRENADES = ['smokegrenade', 'flashbang', 'hegrenade', 'molotov', 'incgrenade', 'decoy'];

/** Stable roster groups: primary, secondary, grenades, objective/tools. Keep duplicate grenades. */
export function rosterEquipment(inventory: string[], active: string, bomb: boolean, defuser: boolean): string[][] {
  const items = [...inventory];
  for (const name of [active, ...(bomb ? ['C4 Explosive'] : []), ...(defuser ? ['Defuse Kit'] : [])]) {
    if (name && !items.some(w => (equipmentId(w) ?? w) === (equipmentId(name) ?? name))) items.push(name);
  }
  const groups: string[][] = [[], [], [], []];
  for (const name of items) {
    const id = equipmentId(name);
    if (id === 'knife') continue;
    let slot = 0;
    if (PISTOLS.has(id ?? '')) slot = 1;
    else if (GRENADES.includes(id ?? '')) slot = 2;
    else if (isUtility(name) || id === 'taser') slot = 3;
    groups[slot]!.push(name);
  }
  groups[2]!.sort((a, b) => GRENADES.indexOf(equipmentId(a) ?? '') - GRENADES.indexOf(equipmentId(b) ?? ''));
  groups[3]!.sort((a, b) => (equipmentId(a) ?? a).localeCompare(equipmentId(b) ?? b));
  return groups;
}

export function equipmentImage(name: string): HTMLImageElement | undefined {
  const src = equipmentIcon(name);
  if (!src || typeof Image === 'undefined') return undefined;
  let image = images.get(src);
  if (!image) {
    image = new Image();
    image.src = src;
    images.set(src, image);
  }
  return image.complete && image.naturalWidth > 0 ? image : undefined;
}
