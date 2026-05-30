import type { Session } from 'better-auth/minimal';

// Better Auth's User type extended with fields added by:
//   - admin plugin: role
//   - additionalFields config: companyId (maps to company_id column)
interface AuthUser {
  id: string;
  email: string;
  name: string;
  emailVerified: boolean;
  image?: string | null;
  createdAt: Date;
  updatedAt: Date;
  role?: string | null;
  companyId?: string | null;
}

// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
  namespace App {
    interface Locals {
      user?: AuthUser | null;
      session?: Session | null;
    }

    // interface Error {}
    // interface PageData {}
    // interface PageState {}
    // interface Platform {}
  }
}

export {};
