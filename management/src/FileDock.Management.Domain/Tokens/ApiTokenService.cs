using System.Security.Cryptography;
using System.Text;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Security;
using FileDock.Management.Domain.Stores;

namespace FileDock.Management.Domain.Tokens;

public sealed record IssuedApiToken(ApiToken Record, string Token);

public sealed class ApiTokenService(IApiTokenStore store, AuditWriter audit)
{
    private static readonly SemaphoreSlim Gate = new(1, 1);

    public async Task<IssuedApiToken> CreateAsync(string name, string scope, Guid createdBy, DateTimeOffset? expiresAt)
    {
        name = name.Trim();
        if (name.Length is < 1 or > 128)
        {
            throw new ArgumentException("Token name must be 1–128 characters.");
        }

        if (scope is not ("ReadOnly" or "Full"))
        {
            throw new ArgumentException("Scope must be ReadOnly or Full.");
        }

        if (expiresAt <= DateTimeOffset.UtcNow)
        {
            throw new ArgumentException("Expiry must be in the future.");
        }

        await Gate.WaitAsync();
        try
        {
            if ((await store.ListAsync()).Any(t => string.Equals(t.Name, name, StringComparison.OrdinalIgnoreCase)))
            {
                throw new InvalidOperationException("That token name is already in use.");
            }

            var secret = Convert.ToHexString(RandomNumberGenerator.GetBytes(32)).ToLowerInvariant();
            var token = new ApiToken
            {
                Id = Guid.CreateVersion7(),
                Name = name,
                Scope = scope,
                CreatedBy = createdBy,
                CreatedAt = DateTimeOffset.UtcNow,
                ExpiresAt = expiresAt,
                SecretHash = Encoding.UTF8.GetBytes(PasswordHasher.Hash(secret)),
            };
            await store.AddAsync(token);
            return new IssuedApiToken(token, $"fdt_{token.Id:N}_{secret}");
        }
        finally { Gate.Release(); }
    }

    public async Task<ApiToken?> AuthenticateAsync(string value)
    {
        var parts = value.Split('_');
        if (parts.Length != 3 || parts[0] != "fdt" || !Guid.TryParseExact(parts[1], "N", out var id)
            || parts[2].Length != 64 || !parts[2].All(char.IsAsciiHexDigit))
        {
            return null;
        }

        var token = await store.FindAsync(id);
        if (token is null || !Active(token))
        {
            return null;
        }

        if (!PasswordHasher.Verify(parts[2], Encoding.UTF8.GetString(token.SecretHash)))
        {
            return null;
        }
        // Re-read after the slow hash so a revocation during verification takes effect.
        await store.ReloadAsync(token);
        return Active(token) ? token : null;
    }

    public async Task AuditUseAsync(ApiToken token, string method, string path)
    {
        await Gate.WaitAsync();
        try
        {
            await store.ReloadAsync(token);
            var now = DateTimeOffset.UtcNow;
            if (token.Scope == "ReadOnly" && method == "GET" && token.LastUsedAt is { } last && now - last < TimeSpan.FromMinutes(1))
            {
                return;
            }

            token.LastUsedAt = now;
            await store.SaveAsync();
            await audit.WriteAsync("ApiToken", token.Id.ToString(), token.Name, "token.use", path, "Succeeded", new { method });
        }
        finally { Gate.Release(); }
    }

    public Task<IReadOnlyList<ApiToken>> ListAsync() => store.ListAsync();
    public async Task RevokeAsync(Guid id)
    {
        await Gate.WaitAsync();
        try
        {
            var token = await store.FindAsync(id) ?? throw new KeyNotFoundException("Token not found.");
            token.RevokedAt ??= DateTimeOffset.UtcNow;
            await store.SaveAsync();
        }
        finally { Gate.Release(); }
    }
    public Task RevokeAllAsync() => store.RevokeAllAsync();
    private static bool Active(ApiToken token) => token.RevokedAt is null && (token.ExpiresAt is null || token.ExpiresAt > DateTimeOffset.UtcNow);
}
