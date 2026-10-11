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
        => db.Sessions.FirstOrDefaultAsync(s => s.TokenHash == tokenHash, cancellationToken);

    public async Task DeleteAsync(Session session, CancellationToken cancellationToken = default)
    {
        db.Sessions.Remove(session);
        await db.SaveChangesAsync(cancellationToken);
    }

    public async Task DeleteForAdminAsync(Guid administratorId, CancellationToken cancellationToken = default)
    {
        await db.Sessions.Where(s => s.AdministratorId == administratorId).ExecuteDeleteAsync(cancellationToken);
    }

    public Task SaveAsync(CancellationToken cancellationToken = default)
        => db.SaveChangesAsync(cancellationToken);
}
