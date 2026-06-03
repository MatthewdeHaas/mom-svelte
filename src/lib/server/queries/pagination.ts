// -------------------------------------------------------------
// Shared pagination utilities for admin list queries.
// Uses OFFSET/LIMIT with URL params (page + per_page).
// -------------------------------------------------------------

export interface PaginationParams {
  page: number;    // 1-based
  perPage: number; // default 50
}

export interface PaginatedResult<T> {
  data: T[];
  total: number;
  page: number;
  perPage: number;
  totalPages: number;
}

/**
 * Parse pagination params from a URL's searchParams.
 * Clamps page >= 1 and perPage between 10 and 100.
 */
export function parsePaginationParams(
  url: URL,
  defaults: { perPage?: number } = {}
): PaginationParams {
  const defaultPerPage = defaults.perPage ?? 50;

  let page = parseInt(url.searchParams.get('page') ?? '1', 10);
  if (isNaN(page) || page < 1) page = 1;

  let perPage = parseInt(url.searchParams.get('per_page') ?? String(defaultPerPage), 10);
  if (isNaN(perPage) || perPage < 10) perPage = 10;
  if (perPage > 100) perPage = 100;

  return { page, perPage };
}

/**
 * Returns LIMIT/OFFSET SQL fragments and the computed offset value.
 * Use the paramIndex to know where to start appending params.
 */
export function paginationSQL(
  params: PaginationParams,
  paramOffset: number
): { sql: string; values: [number, number] } {
  const offset = (params.page - 1) * params.perPage;
  return {
    sql: `LIMIT $${paramOffset} OFFSET $${paramOffset + 1}`,
    values: [params.perPage, offset],
  };
}

/**
 * Build a PaginatedResult from data, total count, and pagination params.
 * Automatically clamps page if it exceeds totalPages.
 */
export function buildPaginatedResult<T>(
  data: T[],
  total: number,
  pagination: PaginationParams
): PaginatedResult<T> {
  const totalPages = Math.max(1, Math.ceil(total / pagination.perPage));
  const page = Math.min(pagination.page, totalPages);

  return {
    data,
    total,
    page,
    perPage: pagination.perPage,
    totalPages,
  };
}
