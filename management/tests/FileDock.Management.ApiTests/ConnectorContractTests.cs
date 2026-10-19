using Filedock.Core.V1;
using FileDock.Management.Api.Endpoints;

namespace FileDock.Management.ApiTests;

public sealed class ConnectorContractTests
{
    [Fact]
    public void OverrideRemovalPreservesGenerationAndNullTarget()
    {
        var body = new ConnectorEndpoints.RoutesInput(
            [new("s3-compatible", "OwnedObjectStorage", "connection", null)], 5);
        var request = ConnectorEndpoints.Request(body);
        Assert.Equal(5UL, request.ExpectedGeneration);
        Assert.Equal("connection", request.Changes[0].Key.ConnectionId);
        Assert.Equal(RouteUse.OwnedObjectStorage, request.Changes[0].Key.Use);
        Assert.Null(request.Changes[0].Target);
        Assert.False(request.AcknowledgedBasicClaimSafety);
        Assert.False(request.AcknowledgedThirdPartyOwnedStorage);
    }

    [Theory]
    [InlineData("not-a-use")]
    [InlineData("Unspecified")]
    [InlineData("999")]
    public void InvalidUsesAreRefused(string use)
    {
        var body = new ConnectorEndpoints.RoutesInput(
            [new("s3-compatible", use, null, new("fd-s3"))], 1);
        Assert.Throws<ArgumentException>(() => ConnectorEndpoints.Request(body));
    }

    [Theory]
    [InlineData(null, false)]
    [InlineData("fd-s3", true)]
    public void TargetMustSelectExactlyOneRecipient(string? connector, bool engine)
    {
        var body = new ConnectorEndpoints.RoutesInput(
            [new("s3-compatible", "BackupRepository", null, new(connector, engine))], 1);
        Assert.Throws<ArgumentException>(() => ConnectorEndpoints.Request(body));
    }

    [Fact]
    public void AcknowledgementsAreForwardedSeparately()
    {
        var body = new ConnectorEndpoints.RoutesInput(
            [new("s3-compatible", "OwnedObjectStorage", null, new("sample-connector"))],
            9, AcknowledgedBasicClaimSafety: true, AcknowledgedThirdPartyOwnedStorage: false);
        var request = ConnectorEndpoints.Request(body);
        Assert.True(request.AcknowledgedBasicClaimSafety);
        Assert.False(request.AcknowledgedThirdPartyOwnedStorage);
        Assert.Equal("sample-connector", request.Changes[0].Target.ConnectorId);
    }
}
