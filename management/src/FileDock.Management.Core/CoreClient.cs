using System.Net.Sockets;
using Filedock.Core.V1;
using Grpc.Core;
using Grpc.Net.Client;
using Microsoft.Extensions.Configuration;

namespace FileDock.Management.Core;

public sealed class CoreClient : ICoreGateway, ICoreEventSource, IOperationGateway, IAsyncDisposable
{
    public const string InterfaceVersion = "1.0";

    private readonly GrpcChannel _channel;

    public CoreControl.CoreControlClient Control { get; }
    public Secrets.SecretsClient Secrets { get; }
    public Observability.ObservabilityClient Observability { get; }
    public Connectors.ConnectorsClient Connectors { get; }
    public Recovery.RecoveryClient Recovery { get; }
    public Extensions.ExtensionsClient Extensions { get; }
    public Storage.StorageClient Storage { get; }

    public CoreClient(IConfiguration configuration)
    {
        var socket = configuration["Core:Socket"] ?? "/run/filedock/core.sock";
        var sockets = new SocketsHttpHandler
        {
            ConnectCallback = async (_, cancellationToken) =>
            {
                var unix = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
                try
                {
                    await unix.ConnectAsync(new UnixDomainSocketEndPoint(socket), cancellationToken);
                    return new NetworkStream(unix, ownsSocket: true);
                }
                catch { unix.Dispose(); throw; }
            },
        };
        _channel = GrpcChannel.ForAddress("http://localhost", new GrpcChannelOptions { HttpHandler = sockets });
        Control = new CoreControl.CoreControlClient(_channel);
        Secrets = new Secrets.SecretsClient(_channel);
        Observability = new Observability.ObservabilityClient(_channel);
        Connectors = new Connectors.ConnectorsClient(_channel);
        Recovery = new Recovery.RecoveryClient(_channel);
        Extensions = new Extensions.ExtensionsClient(_channel);
        Storage = new Storage.StorageClient(_channel);
    }

    public async Task ApplyApplianceNameAsync(string applianceName, string actor, string requestId, CancellationToken cancellationToken = default)
    {
        var headers = Metadata(actor, requestId, true);
        var reference = new ResourceRef { Kind = ResourceKind.Settings, Id = "default" };
        ulong generation = 0;
        var spec = new Google.Protobuf.WellKnownTypes.Struct();
        try
        {
            var existing = await Control.GetResourceAsync(reference, headers: Metadata(actor, requestId, false), cancellationToken: cancellationToken);
            generation = existing.Generation;
            spec = existing.Spec.Clone();
        }
        catch (RpcException)
        {
            generation = 0;
        }

        spec.Fields["applianceName"] = Google.Protobuf.WellKnownTypes.Value.ForString(applianceName);
        await Control.ApplyChangeAsync(
            new ApplyChangeRequest
            {
                Changes =
                {
                    new ResourceChange
                    {
                        Upsert = new ResourceChange.Types.Upsert
                        {
                            Ref = reference,
                            ExpectedGeneration = generation,
                            Spec = spec,
                        },
                    },
                },
            },
            headers: headers,
            cancellationToken: cancellationToken);
    }

    public async Task<GeneratedRecoveryKeyDto> GenerateRecoveryKeyAsync(string actor, string requestId, CancellationToken cancellationToken = default)
    {
        var result = await Recovery.GenerateRecoveryKeyAsync(
            new GenerateRecoveryKeyRequest(),
            headers: Metadata(actor, requestId, true),
            cancellationToken: cancellationToken);
        return new GeneratedRecoveryKeyDto(result.KeyId, result.RecoveryKey);
    }

    public async Task ConfirmRecoveryKeyAsync(string keyId, string actor, string requestId, CancellationToken cancellationToken = default)
    {
        var request = new ConfirmRecoveryKeyRequest { KeyId = keyId };
        await Recovery.ConfirmRecoveryKeyAsync(request, headers: Metadata(actor, requestId, true), cancellationToken: cancellationToken);
    }

    public async Task<SystemStatusDto> GetSystemStatusAsync(CancellationToken cancellationToken = default)
    {
        var status = await Control.GetSystemStatusAsync(
            new GetSystemStatusRequest(),
            headers: Metadata("management", Guid.NewGuid().ToString("N"), false),
            cancellationToken: cancellationToken);
        var components = status.Components.Select(c =>
            new ComponentHealthDto(c.Id, c.Name, c.State.ToString(), c.Detail)).ToList();
        return new SystemStatusDto(
            status.InterfaceVersion,
            status.ReleaseVersion,
            status.ApplianceName,
            components,
            status.StateProtectionWarning,
            status.NotificationChannelMissing);
    }

    public async IAsyncEnumerable<CoreEventDto> StreamEventsAsync(ulong afterSeq, Action connected, [System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken cancellationToken)
    {
        using var call = Control.StreamEvents(new StreamEventsRequest { AfterSeq = afterSeq }, headers: Metadata("management", "events", false), cancellationToken: cancellationToken);
        await call.ResponseHeadersAsync;
        connected();
        await foreach (var change in call.ResponseStream.ReadAllAsync(cancellationToken))
        {
            using var payload = System.Text.Json.JsonDocument.Parse(change.Payload?.ToString() ?? "{}");
            yield return new(change.Seq, change.At?.ToDateTimeOffset(), change.Type, change.Subject?.Kind.ToString(), change.Subject?.Id, payload.RootElement.Clone());
        }
    }
    public async Task<OperationPageDto> ListOperationsAsync(bool activeOnly, uint pageSize, string? pageToken, CancellationToken cancellationToken)
    {
        var response = await Control.ListOperationsAsync(new ListOperationsRequest { ActiveOnly = activeOnly, Page = new PageRequest { PageSize = pageSize, PageToken = pageToken ?? "" } }, headers: Metadata("management", "operations", false), cancellationToken: cancellationToken);
        return new(response.Operations.Select(ToOperation).ToArray(), string.IsNullOrEmpty(response.NextPageToken) ? null : response.NextPageToken);
    }
    public async Task<OperationDto> GetOperationAsync(string id, CancellationToken cancellationToken)
        => ToOperation(await Control.GetOperationAsync(new OperationRef { Id = id }, headers: Metadata("management", "operations", false), cancellationToken: cancellationToken));
    public async Task<OperationDto> ReconcileAllAsync(string actor, string requestId, CancellationToken cancellationToken)
        => ToOperation(await Control.StartOperationAsync(new StartOperationRequest { Kind = "ReconcileAll" }, headers: Metadata(actor, requestId, true), cancellationToken: cancellationToken));
    public static OperationDto ToOperation(Operation operation) => new(operation.Id, operation.Kind, operation.State.ToString(), operation.Steps.Select(s => new OperationStepDto(s.Name, s.State, s.Attempts, s.Detail)).ToArray(), operation.StartedBy, operation.StartedAt?.ToDateTimeOffset(), operation.FinishedAt?.ToDateTimeOffset(), string.IsNullOrEmpty(operation.Error) ? null : operation.Error);

    public static Metadata Metadata(string actor, string requestId, bool mutating)
    {
        var meta = new Metadata { { "fd-interface-version", InterfaceVersion } };
        if (mutating)
        {
            meta.Add("fd-actor", actor);
            meta.Add("fd-request-id", requestId);
        }

        return meta;
    }

    public static (int Status, string Detail) ToProblem(RpcException error)
    {
        var status = error.StatusCode switch
        {
            StatusCode.InvalidArgument => 400,
            StatusCode.NotFound => 404,
            StatusCode.AlreadyExists => 409,
            StatusCode.Aborted => 409,
            StatusCode.FailedPrecondition => 412,
            StatusCode.PermissionDenied => 403,
            StatusCode.Unauthenticated => 401,
            StatusCode.ResourceExhausted => 507,
            StatusCode.Unavailable or StatusCode.DeadlineExceeded => 503,
            _ => 502,
        };
        return (status, error.Status.Detail);
    }

    public ValueTask DisposeAsync()
    {
        _channel.Dispose();
        return ValueTask.CompletedTask;
    }
}
