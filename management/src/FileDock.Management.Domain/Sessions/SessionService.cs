using System.Security.Cryptography;
using FileDock.Management.Domain.Stores;

namespace FileDock.Management.Domain.Sessions;

public sealed record IssuedSession(string Token, string Csrf, Session Session);

public sealed class SessionService(ISessionStore store)
{
    public static readonly TimeSpan IdleExpiry = TimeSpan.FromMinutes(30);

    public async Task<IssuedSession> CreateAsync(Administrator admin, string? clientIp, string? userAgent, CancellationToken cancellationToken = default)
    {
        var token = Convert.ToHexString(RandomNumberGenerator.GetBytes(32)).ToLowerInvariant();
        var csrf = Convert.ToHexString(RandomNumberGenerator.GetBytes(16)).ToLowerInvariant();
        var session = new Session
        {
            Id = Guid.CreateVersion7(),
            AdministratorId = admin.Id,
            TokenHash = SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(token)),
            CreatedAt = DateTimeOffset.UtcNow,
            LastSeenAt = DateTimeOffset.UtcNow,
            ClientIp = clientIp,
            UserAgent = userAgent,
            CsrfToken = csrf,
        };
        await store.AddAsync(session, cancellationToken);
        return new IssuedSession(token, csrf, session);
    }

    public async Task<Session?> AuthenticateAsync(string token, CancellationToken cancellationToken = default)
    {
        var hash = SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(token));
        var session = await store.FindByTokenHashAsync(hash, cancellationToken);
        if (session is null)
        {
            return null;
        }

        if (DateTimeOffset.UtcNow - session.LastSeenAt > IdleExpiry)
        {
            await store.DeleteAsync(session, cancellationToken);
            return null;
        }

        session.LastSeenAt = DateTimeOffset.UtcNow;
        await store.SaveAsync(cancellationToken);
        return session;
    }

    public async Task SignOutAsync(Session session, CancellationToken cancellationToken = default)
    {
        await store.DeleteAsync(session, cancellationToken);
    }

    public async Task SignOutAllAsync(Guid administratorId, CancellationToken cancellationToken = default)
    {
        await store.DeleteForAdminAsync(administratorId, cancellationToken);
    }
}
