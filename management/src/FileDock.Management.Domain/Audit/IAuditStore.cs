namespace FileDock.Management.Domain.Audit;

public interface IAuditStore
{
    Task AddAsync(AuditEntry entry, CancellationToken cancellationToken = default);
    Task DeleteOlderThanAsync(DateTimeOffset cutoff, CancellationToken cancellationToken = default);
}
