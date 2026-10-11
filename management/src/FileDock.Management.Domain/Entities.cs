namespace FileDock.Management.Domain;

public sealed class Administrator
{
    public Guid Id { get; set; }
    public required string Username { get; set; }
    public required string PasswordHash { get; set; }
    public bool Enabled { get; set; } = true;
    public int FailedAttempts { get; set; }
    public DateTimeOffset? LockedUntil { get; set; }
    public DateTimeOffset? LastSignInAt { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
    public DateTimeOffset UpdatedAt { get; set; }
}

public sealed class Session
{
    public Guid Id { get; set; }
    public Guid AdministratorId { get; set; }
    public required byte[] TokenHash { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
    public DateTimeOffset LastSeenAt { get; set; }
    public string? ClientIp { get; set; }
    public string? UserAgent { get; set; }
    public string CsrfToken { get; set; } = "";
}

public sealed class ApiToken
{
    public Guid Id { get; set; }
    public required string Name { get; set; }
    public required string Scope { get; set; }
    public required byte[] SecretHash { get; set; }
    public Guid CreatedBy { get; set; }
    public DateTimeOffset? ExpiresAt { get; set; }
    public DateTimeOffset? RevokedAt { get; set; }
    public DateTimeOffset? LastUsedAt { get; set; }
    public DateTimeOffset CreatedAt { get; set; }
}

public sealed class AuditEntry
{
    public Guid Id { get; set; }
    public DateTimeOffset At { get; set; }
    public required string ActorKind { get; set; }
    public required string ActorId { get; set; }
    public required string ActorName { get; set; }
    public required string Action { get; set; }
    public required string Target { get; set; }
    public required string Result { get; set; }
    public string Detail { get; set; } = "{}";
}

public sealed class SetupState
{
    public int Id { get; set; } = 1;
    public required string Status { get; set; }
    public byte[]? SetupCodeHash { get; set; }
    public DateTimeOffset SetupCodeIssuedAt { get; set; }
    public string AttemptCounters { get; set; } = "{}";
}

public sealed class Settings
{
    public int Id { get; set; } = 1;
    public string? ListenAddresses { get; set; }
}
