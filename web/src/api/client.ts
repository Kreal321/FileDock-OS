export type Problem = {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  errors?: { code: string; field: string; message: string; conflicting?: string }[];
};

export class ApiError extends Error {
  constructor(
    readonly problem: Problem,
    readonly status: number,
  ) {
    super(problem.detail ?? problem.title ?? `HTTP ${status}`);
  }
}

export type ApiClientOptions = {
  baseUrl?: string;
  csrfToken?: string;
  notifySessionExpiry?: boolean;
};

export function createApiClient(options: ApiClientOptions = {}) {
  const baseUrl = options.baseUrl ?? '/api/v1';

  async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const headers = new Headers(init.headers);
    if (init.method && init.method !== 'GET' && init.method !== 'HEAD') {
      headers.set('X-CSRF-Token', options.csrfToken ?? csrfFromCookie());
      if (!headers.has('content-type')) {
        headers.set('content-type', 'application/json');
      }
    }
    const response = await fetch(`${baseUrl}${path}`, {
      ...init,
      headers,
      credentials: 'same-origin',
    });
    if (!response.ok) {
      if (
        response.status === 401 &&
        options.notifySessionExpiry !== false &&
        path !== '/auth/sign-in' &&
        path !== '/setup/recovery-key/confirm'
      ) {
        window.dispatchEvent(new Event('filedock:session-expired'));
      }
      const problem = (await response.json().catch(() => ({
        status: response.status,
        title: response.statusText,
      }))) as Problem;
      throw new ApiError(problem, response.status);
    }
    if (response.status === 204) {
      return undefined as T;
    }
    return (await response.json()) as T;
  }

  return {
    get: <T>(path: string) => request<T>(path),
    post: <T>(path: string, body?: unknown) =>
      request<T>(path, {
        method: 'POST',
        body: body === undefined ? undefined : JSON.stringify(body),
      }),
    patch: <T>(path: string, body: unknown, etag?: string) =>
      request<T>(path, {
        method: 'PATCH',
        body: JSON.stringify(body),
        headers: etag ? { 'If-Match': etag } : undefined,
      }),
    delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
    put: <T>(path: string, body: unknown, etag?: string) =>
      request<T>(path, {
        method: 'PUT',
        body: JSON.stringify(body),
        headers: etag ? { 'If-Match': etag } : undefined,
      }),
    del: <T>(path: string, etag?: string) =>
      request<T>(path, { method: 'DELETE', headers: etag ? { 'If-Match': etag } : undefined }),
  };
}

export const api = createApiClient();

function csrfFromCookie(): string {
  const match = document.cookie.match(/(?:^|; )fd_http_csrf=([^;]*)/);
  return match?.[1] ? decodeURIComponent(match[1]) : '';
}

export function hasCsrfCookie(): boolean {
  return csrfFromCookie().length > 0;
}
