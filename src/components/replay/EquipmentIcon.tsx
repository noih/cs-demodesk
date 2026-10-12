import { equipmentIcon, isUtility } from '../../replay/equipment.ts';

export function EquipmentIcon({ name }: { name: string }) {
  const src = equipmentIcon(name);
  return src
    ? <img className={`equipment-icon ${isUtility(name) ? 'utility' : ''}`} src={src} alt={name} title={name} draggable={false} />
    : <span className="equipment-unknown" title={name} aria-label={name}>?</span>;
}
