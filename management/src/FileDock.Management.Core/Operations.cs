namespace FileDock.Management.Core;

public sealed record OperationStepDto(string Name, string State, uint Attempts, string Detail);
public sealed record OperationDto(string Id, string Kind, string State, IReadOnlyList<OperationStepDto> Steps, string StartedBy, DateTimeOffset? StartedAt, DateTimeOffset? FinishedAt, string? Error);
public sealed record OperationPageDto(IReadOnlyList<OperationDto> Operations, string? NextPageToken);
public interface IOperationGateway
{
    Task<OperationPageDto> ListOperationsAsync(bool activeOnly, uint pageSize, string? pageToken, CancellationToken cancellationToken);
    Task<OperationDto> GetOperationAsync(string id, CancellationToken cancellationToken);
    Task<OperationDto> ReconcileAllAsync(string actor, string requestId, CancellationToken cancellationToken);
}
