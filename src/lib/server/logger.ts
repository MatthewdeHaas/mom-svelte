import pino from 'pino';
import { dev } from '$app/environment';

const logger = pino({
  // Override with LOG_LEVEL env var if set; otherwise debug in dev, info in prod.
  level: process.env.LOG_LEVEL ?? (dev ? 'debug' : 'info'),
  ...(dev
    ? {
        transport: {
          target: 'pino-pretty',
          options: { colorize: true },
        },
      }
    : {}),
});

export default logger;
