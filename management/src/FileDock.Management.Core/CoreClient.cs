using System.Net.Sockets;
using Filedock.Core.V1;
using Grpc.Core;
using Grpc.Net.Client;
using Microsoft.Extensions.Configuration;

namespace FileDock.Management.Core;

public sealed class CoreClient : ICoreGateway, IAsyncDisposable
{
    public const string InterfaceVersion = "1.0";

    private readonly GrpcChannel _channel;

    public CoreControl.CoreControlClient Control { get; }
    public Secrets.SecretsClient Secrets { get; }
    public Observability.ObservabilityClient Observability { get; }
    public Connectors.ConnectorsClient Connectors { get; }
    public Recovery.RecoveryClient Recovery { get; }

    public CoreClient(IConfiguration configuration)
    {
        var socket = configuration["Core:Socket"] ?? "/run/filedock/core.sock";
        var sockets = new SocketsHttpHandler
        {
            ConnectCallback = async (_, cancellationToken) =>
            {
                var unix = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
                await unix.ConnectAsync(new UnixDomainSocketEndPoint(socket), cancellationToken);
                return new NetworkStream(unix, ownsSocket: true);
            },
        };
        _channel = GrpcChannel.ForAddress("http://localhost", new GrpcChannelOptions { HttpHandler = sockets });
        Control = new CoreControl.CoreControlClient(_channel);
        Secrets = new Secrets.SecretsClient(_channel);
        Observability = new Observability.ObservabilityClient(_channel);
        Connectors = new Connectors.ConnectorsClient(_channel);
        Recovery = new Recovery.RecoveryClient(_channel);
    }

    public async Task ApplyApplianceNameAsync(string applianceName, string actor, string requestId, CancellationToken cancellationToken = default)
    {
        var headers = Metadata(actor, requestId, true);
        var reference = new ResourceRef { Kind = ResourceKind.Settings, Id = "default" };
        ulong generation = 0;
        try
        {
            var existing = await Control.GetResourceAsync(reference, headers: Metadata(actor, requestId, false), cancellationToken: cancellationToken);
            generation = existing.Generation;
        }
        catch (RpcException)
        {
            generation = 0;
        }

        var spec = Google.Protobuf.WellKnownTypes.Struct.Parser.ParseJson(
            System.Text.Json.JsonSerializer.Serialize(new { applianceName }));
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
        return new GeneratedRecoveryKeyDto(result.KeyId, result.RecoveryKey, result.ConfirmGroupIndexes.ToArray());
    }

    public async Task ConfirmRecoveryKeyAsync(string keyId, IReadOnlyDictionary<uint, string> groups, string actor, string requestId, CancellationToken cancellationToken = default)
    {
        var request = new ConfirmRecoveryKeyRequest { KeyId = keyId };
        foreach (var (index, value) in groups)
        {
            request.Groups[index] = value;
        }

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
