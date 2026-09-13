// Mirrors the application-wide Rust ErrorCode wire values.
export const ERROR_CODES = { APP_CLOSED: 'app-closed' } as const;
export type ErrorCode = typeof ERROR_CODES[keyof typeof ERROR_CODES];
