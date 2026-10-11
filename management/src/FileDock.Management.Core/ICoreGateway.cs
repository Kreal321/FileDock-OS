namespace FileDock.Management.Core;

public sealed record GeneratedRecoveryKeyDto(string KeyId, string RecoveryKey, IReadOnlyList<uint> ConfirmGroupIndexes);

public sealed record ComponentHealthDto(string Id, string Name, string State, string Detail);

public sealed record SystemStatusDto(
    string InterfaceVersion,
    string ReleaseVersion,
    string ApplianceName,
    IReadOnlyList<ComponentHealthDto> Components,
    bool StateProtectionWarning,
    bool NotificationChannelMissing);

public interface ICoreGateway
{
    Task ApplyApplianceNameAsync(string applianceName, string actor, string requestId, CancellationToken cancellationToken = default);
    Task<GeneratedRecoveryKeyDto> GenerateRecoveryKeyAsync(string actor, string requestId, CancellationToken cancellationToken = default);
    Task ConfirmRecoveryKeyAsync(string keyId, IReadOnlyDictionary<uint, string> groups, string actor, string requestId, CancellationToken cancellationToken = default);
    Task<SystemStatusDto> GetSystemStatusAsync(CancellationToken cancellationToken = default);
}
