using FileDock.Management.Domain;
using FileDock.Management.Domain.Stores;
using Microsoft.EntityFrameworkCore;

namespace FileDock.Management.Data;

public sealed class EfSetupStore(AppDbContext db) : ISetupStore
{
    public async Task<SetupState> GetAsync(CancellationToken cancellationToken = default)
    {
        var state = await db.SetupStates.SingleOrDefaultAsync(s => s.Id == 1, cancellationToken);
        if (state is not null)
        {
            return state;
        }

        state = new SetupState { Id = 1, Status = "AwaitingSetup" };
        db.SetupStates.Add(state);
        await db.SaveChangesAsync(cancellationToken);
        return state;
    }

    public Task SaveAsync(SetupState state, CancellationToken cancellationToken = default)
    {
        db.SetupStates.Update(state);
        return db.SaveChangesAsync(cancellationToken);
    }
}

public sealed class EfAdminStore(AppDbContext db) : IAdminStore
{
    public Task<Administrator?> FindByUsernameAsync(string username, CancellationToken cancellationToken = default)
        => db.Administrators.FirstOrDefaultAsync(a => a.Username == username, cancellationToken);

    public Task<Administrator?> FindByIdAsync(Guid id, CancellationToken cancellationToken = default)
        => db.Administrators.FirstOrDefaultAsync(a => a.Id == id, cancellationToken);

    public Task<int> CountEnabledAsync(CancellationToken cancellationToken = default)
        => db.Administrators.CountAsync(a => a.Enabled, cancellationToken);

    public async Task AddAsync(Administrator admin, CancellationToken cancellationToken = default)
    {
        db.Administrators.Add(admin);
        await db.SaveChangesAsync(cancellationToken);
    }

    public async Task DeleteAsync(Administrator admin, CancellationToken cancellationToken = default)
    {
        db.Administrators.Remove(admin);
        await db.SaveChangesAsync(cancellationToken);
    }

    public Task SaveAsync(CancellationToken cancellationToken = default)
        => db.SaveChangesAsync(cancellationToken);

    public async Task<IReadOnlyList<Administrator>> ListAsync(CancellationToken cancellationToken = default)
        => await db.Administrators.AsNoTracking().ToListAsync(cancellationToken);
}

public sealed class EfSessionStore(AppDbContext db) : ISessionStore
{
    public async Task AddAsync(Session session, CancellationToken cancellationToken = default)
    {
        db.Sessions.Add(session);
        await db.SaveChangesAsync(cancellationToken);
    }

    public Task<Session?> FindByTokenHashAsync(byte[] tokenHash, CancellationToken cancellationToken = default)
        => db.Sessions.AsNoTracking().FirstOrDefaultAsync(s => s.TokenHash == tokenHash, cancellationToken);

    public async Task DeleteAsync(Session session, CancellationToken cancellationToken = default)
    {
        await db.Sessions.Where(s => s.Id == session.Id).ExecuteDeleteAsync(cancellationToken);
    }

    public async Task DeleteForAdminAsync(Guid administratorId, CancellationToken cancellationToken = default)
    {
        await db.Sessions.Where(s => s.AdministratorId == administratorId).ExecuteDeleteAsync(cancellationToken);
    }

    public async Task<bool> TouchAsync(Guid id, DateTimeOffset at, CancellationToken cancellationToken = default)
        => await db.Sessions.Where(s => s.Id == id).ExecuteUpdateAsync(update => update.SetProperty(s => s.LastSeenAt, at), cancellationToken) == 1;
}

public sealed class EfApiTokenStore(AppDbContext db) : IApiTokenStore
{
    public async Task AddAsync(ApiToken token) { db.ApiTokens.Add(token); await db.SaveChangesAsync(); }
    public Task<ApiToken?> FindAsync(Guid id) => db.ApiTokens.SingleOrDefaultAsync(t => t.Id == id);
    public Task ReloadAsync(ApiToken token) => db.Entry(token).ReloadAsync();
    public async Task<IReadOnlyList<ApiToken>> ListAsync() => await db.ApiTokens.AsNoTracking().OrderBy(t => t.Name).ToListAsync();
    public Task SaveAsync() => db.SaveChangesAsync();
    public Task RevokeAllAsync() => db.ApiTokens.Where(t => t.RevokedAt == null)
        .ExecuteUpdateAsync(s => s.SetProperty(t => t.RevokedAt, DateTimeOffset.UtcNow));
}
