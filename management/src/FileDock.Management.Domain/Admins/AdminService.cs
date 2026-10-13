using FileDock.Management.Domain.Security;
using FileDock.Management.Domain.Stores;

namespace FileDock.Management.Domain.Admins;

public sealed class LastAdminException : Exception
{
    public LastAdminException() : base("The last enabled administrator cannot be disabled or removed.") { }
}

public sealed class AccountLockedException : Exception
{
    public AccountLockedException(DateTimeOffset until) : base($"Account locked until {until:O}.")
    {
        Until = until;
    }

    public DateTimeOffset Until { get; }
}

public sealed class AdminService(IAdminStore store)
{
    private static readonly SemaphoreSlim Gate = new(1, 1);
    public async Task<Administrator> CreateAsync(string username, string password, CancellationToken cancellationToken = default)
    {
        await Gate.WaitAsync(cancellationToken);
        try { return await CreateInternalAsync(username, password, cancellationToken); }
        finally { Gate.Release(); }
    }
    public async Task DisableAsync(Guid id, CancellationToken cancellationToken = default)
    {
        await UpdateAsync(id, null, false, cancellationToken);
    }
    public async Task DeleteAsync(Guid id, CancellationToken cancellationToken = default)
    {
        await Gate.WaitAsync(cancellationToken);
        try { await DeleteInternalAsync(id, cancellationToken); }
        finally { Gate.Release(); }
    }
    public async Task ChangePasswordAsync(Guid id, string password, CancellationToken cancellationToken = default)
    {
        await Gate.WaitAsync(cancellationToken);
        try { await ChangePasswordInternalAsync(id, password, cancellationToken); }
        finally { Gate.Release(); }
    }
    public async Task<Administrator> AuthenticateAsync(string username, string password, CancellationToken cancellationToken = default)
    {
        await Gate.WaitAsync(cancellationToken);
        try { return await AuthenticateInternalAsync(username, password, cancellationToken); }
        finally { Gate.Release(); }
    }
    public async Task UpdateAsync(Guid id, string? username, bool? enabled, CancellationToken cancellationToken = default)
    {
        await Gate.WaitAsync(cancellationToken);
        try
        {
            var admin = await store.FindByIdAsync(id, cancellationToken) ?? throw new KeyNotFoundException("Administrator not found.");
            if (enabled == false && admin.Enabled && await store.CountEnabledAsync(cancellationToken) <= 1)
            {
                throw new LastAdminException();
            }

            if (username is not null)
            {
                var normalized = Normalize(username);
                if (normalized.Length is < 1 or > 64)
                {
                    throw new ArgumentException("Username must be 1–64 characters.");
                }

                var other = await store.FindByUsernameAsync(normalized, cancellationToken);
                if (other is not null && other.Id != id)
                {
                    throw new InvalidOperationException("That username is already in use.");
                }

                admin.Username = normalized;
            }
            if (enabled is not null)
            {
                admin.Enabled = enabled.Value;
            }

            admin.UpdatedAt = DateTimeOffset.UtcNow;
            await store.SaveAsync(cancellationToken);
        }
        finally { Gate.Release(); }
    }

    private async Task<Administrator> CreateInternalAsync(string username, string password, CancellationToken cancellationToken = default)
    {
        PasswordHasher.EnsurePolicy(password);
        var normalized = Normalize(username);
        if (normalized.Length is < 1 or > 64)
        {
            throw new ArgumentException("Username must be 1–64 characters.", nameof(username));
        }

        if (await store.FindByUsernameAsync(normalized, cancellationToken) is not null)
        {
            throw new InvalidOperationException("That username is already in use.");
        }

        var admin = new Administrator
        {
            Id = Guid.CreateVersion7(),
            Username = normalized,
            PasswordHash = PasswordHasher.Hash(password),
            Enabled = true,
            CreatedAt = DateTimeOffset.UtcNow,
            UpdatedAt = DateTimeOffset.UtcNow,
        };
        await store.AddAsync(admin, cancellationToken);
        return admin;
    }

    private async Task DeleteInternalAsync(Guid id, CancellationToken cancellationToken = default)
    {
        var admin = await store.FindByIdAsync(id, cancellationToken)
            ?? throw new KeyNotFoundException("Administrator not found.");
        if (admin.Enabled && await store.CountEnabledAsync(cancellationToken) <= 1)
        {
            throw new LastAdminException();
        }

        await store.DeleteAsync(admin, cancellationToken);
    }

    private async Task ChangePasswordInternalAsync(Guid id, string password, CancellationToken cancellationToken = default)
    {
        PasswordHasher.EnsurePolicy(password);
        var admin = await store.FindByIdAsync(id, cancellationToken)
            ?? throw new KeyNotFoundException("Administrator not found.");
        admin.PasswordHash = PasswordHasher.Hash(password);
        admin.FailedAttempts = 0;
        admin.LockedUntil = null;
        admin.UpdatedAt = DateTimeOffset.UtcNow;
        await store.SaveAsync(cancellationToken);
    }

    private async Task<Administrator> AuthenticateInternalAsync(string username, string password, CancellationToken cancellationToken = default)
    {
        var admin = await store.FindByUsernameAsync(Normalize(username), cancellationToken);
        if (admin is null || !admin.Enabled)
        {
            throw new UnauthorizedAccessException("Invalid username or password.");
        }

        if (admin.LockedUntil is { } until && until > DateTimeOffset.UtcNow)
        {
            throw new AccountLockedException(until);
        }

        if (!PasswordHasher.Verify(password, admin.PasswordHash))
        {
            admin.FailedAttempts += 1;
            if (admin.FailedAttempts >= 5)
            {
                admin.LockedUntil = DateTimeOffset.UtcNow.AddMinutes(15);
                admin.FailedAttempts = 0;
            }

            admin.UpdatedAt = DateTimeOffset.UtcNow;
            await store.SaveAsync(cancellationToken);
            if (admin.LockedUntil is { } locked && locked > DateTimeOffset.UtcNow)
            {
                throw new AccountLockedException(locked);
            }

            throw new UnauthorizedAccessException("Invalid username or password.");
        }

        admin.FailedAttempts = 0;
        admin.LockedUntil = null;
        admin.LastSignInAt = DateTimeOffset.UtcNow;
        admin.UpdatedAt = DateTimeOffset.UtcNow;
        await store.SaveAsync(cancellationToken);
        return admin;
    }

    private static string Normalize(string username) => username.Trim().ToLowerInvariant();
}
