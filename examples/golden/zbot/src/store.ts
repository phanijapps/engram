export interface Ledger {
  /** Record a settled payment. */
  settle(n: number): number;
}

export const MAX_RETRIES = 3;

const retry = (n: number): number => {
  return n + MAX_RETRIES;
};

export function settle(n: number): number {
  return retry(n);
}
