import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { AlertDialog, Button, Callout, Flex, IconButton } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';

type Color = 'red' | 'green' | 'amber';
const Notifications = createContext<{ host: HTMLDivElement | null; notify: (message: string, color?: Color) => void } | null>(null);

export function NotificationProvider({ children }: { children: ReactNode }) {
  const [host, setHost] = useState<HTMLDivElement | null>(null);
  const [notices, setNotices] = useState<{ id: number; message: string; color: Color }[]>([]);
  const nextId = useRef(0);
  const notify = useCallback((message: string, color: Color = 'red') => {
    const id = nextId.current++;
    setNotices(items => [...items, { id, message, color }]);
  }, []);
  return <Notifications.Provider value={{ host, notify }}>
    {children}
    <div ref={setHost} className="notification-viewport" />
    {notices.map(notice => <Toast key={notice.id} {...notice} duration={notice.color === 'green' ? 3000 : 0}
      onDismiss={() => setNotices(items => items.filter(item => item.id !== notice.id))} />)}
  </Notifications.Provider>;
}

export function useNotify() {
  const context = useContext(Notifications);
  if (!context) throw new Error('NotificationProvider is required');
  return context.notify;
}

export function Toast({ message, color, onDismiss, action, duration = 0 }: {
  message: string; color: Color; onDismiss: () => void; action?: ReactNode; duration?: number;
}) {
  const { t } = useTranslation();
  const context = useContext(Notifications);
  const dismiss = useRef(onDismiss);
  useEffect(() => { dismiss.current = onDismiss; }, [onDismiss]);
  useEffect(() => {
    if (!duration) return;
    const timer = setTimeout(() => dismiss.current(), duration);
    return () => clearTimeout(timer);
  }, [message, duration]);
  if (!context?.host) return null;
  return createPortal(<Callout.Root className="app-toast" size="1" color={color} variant="surface"
    role={color === 'red' ? 'alert' : 'status'} aria-live={color === 'red' ? 'assertive' : 'polite'} aria-atomic="true">
    <Callout.Text>{message}</Callout.Text>
    {action}
    <IconButton size="2" variant="ghost" aria-label={t('common.close')} onClick={onDismiss}>
      <i aria-hidden="true" className="bi bi-x-lg app-icon" />
    </IconButton>
  </Callout.Root>, context.host);
}

export function ConfirmDialog({ open, onOpenChange, trigger, title, description, confirmLabel, onConfirm }: {
  open?: boolean; onOpenChange?: (open: boolean) => void; trigger?: ReactNode;
  title: string; description: ReactNode; confirmLabel: string; onConfirm: () => void;
}) {
  const { t } = useTranslation();
  return <AlertDialog.Root open={open} onOpenChange={onOpenChange}>
    {trigger && <AlertDialog.Trigger>{trigger}</AlertDialog.Trigger>}
    <AlertDialog.Content maxWidth="480px">
      <AlertDialog.Title>{title}</AlertDialog.Title>
      <AlertDialog.Description size="2">{description}</AlertDialog.Description>
      <Flex gap="3" mt="4" justify="end">
        <AlertDialog.Cancel><Button variant="soft" color="gray">{t('common.cancel')}</Button></AlertDialog.Cancel>
        <AlertDialog.Action><Button color="red" onClick={onConfirm}>{confirmLabel}</Button></AlertDialog.Action>
      </Flex>
    </AlertDialog.Content>
  </AlertDialog.Root>;
}
