namespace FileDock.Management.Domain.Stores;

public interface ISetupStore
{
    Task<SetupState> GetAsync(CancellationToken cancellationToken = default);
    Task SaveAsync(SetupState state, CancellationToken cancellationToken = default);
}

public interface IAdminStore
{
    Task<Administrator?> FindByUsernameAsync(string username, CancellationToken cancellationToken = default);
    Task<Administrator?> FindByIdAsync(Guid id, CancellationToken cancellationToken = default);
    Task<int> CountEnabledAsync(CancellationToken cancellationToken = default);
    Task AddAsync(Administrator admin, CancellationToken cancellationToken = default);
    Task SaveAsync(CancellationToken cancellationToken = default);
    Task<IReadOnlyList<Administrator>> ListAsync(CancellationToken cancellationToken = default);
}

public interface ISessionStore
{
    Task AddAsync(Session session, CancellationToken cancellationToken = default);
    Task<Session?> FindByTokenHashAsync(byte[] tokenHash, CancellationToken cancellationToken = default);
    Task DeleteAsync(Session session, CancellationToken cancellationToken = default);
    Task DeleteForAdminAsync(Guid administratorId, CancellationToken cancellationToken = default);
    Task SaveAsync(CancellationToken cancellationToken = default);
}
