#pragma warning disable CA1861
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace FileDock.Management.Data.Migrations;

public partial class Initial : Migration
{
    protected override void Up(MigrationBuilder migrationBuilder)
    {
        migrationBuilder.CreateTable(
            name: "Administrators",
            columns: table => new
            {
                Id = table.Column<Guid>(type: "TEXT", nullable: false),
                Username = table.Column<string>(type: "TEXT", maxLength: 64, nullable: false),
                PasswordHash = table.Column<string>(type: "TEXT", nullable: false),
                Enabled = table.Column<bool>(type: "INTEGER", nullable: false),
                FailedAttempts = table.Column<int>(type: "INTEGER", nullable: false),
                LockedUntil = table.Column<DateTimeOffset>(type: "TEXT", nullable: true),
                LastSignInAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: true),
                CreatedAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: false),
                UpdatedAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: false),
            },
            constraints: table => table.PrimaryKey("PK_Administrators", x => x.Id));

        migrationBuilder.CreateIndex(name: "IX_Administrators_Username", table: "Administrators", column: "Username", unique: true);

        migrationBuilder.CreateTable(
            name: "Sessions",
            columns: table => new
            {
                Id = table.Column<Guid>(type: "TEXT", nullable: false),
                AdministratorId = table.Column<Guid>(type: "TEXT", nullable: false),
                TokenHash = table.Column<byte[]>(type: "BLOB", nullable: false),
                CreatedAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: false),
                LastSeenAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: false),
                ClientIp = table.Column<string>(type: "TEXT", nullable: true),
                UserAgent = table.Column<string>(type: "TEXT", nullable: true),
            },
            constraints: table => table.PrimaryKey("PK_Sessions", x => x.Id));

        migrationBuilder.CreateTable(
            name: "ApiTokens",
            columns: table => new
            {
                Id = table.Column<Guid>(type: "TEXT", nullable: false),
                Name = table.Column<string>(type: "TEXT", nullable: false),
                Scope = table.Column<string>(type: "TEXT", nullable: false),
                SecretHash = table.Column<byte[]>(type: "BLOB", nullable: false),
                CreatedBy = table.Column<Guid>(type: "TEXT", nullable: false),
                ExpiresAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: true),
                RevokedAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: true),
                LastUsedAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: true),
                CreatedAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: false),
            },
            constraints: table => table.PrimaryKey("PK_ApiTokens", x => x.Id));

        migrationBuilder.CreateIndex(name: "IX_ApiTokens_Name", table: "ApiTokens", column: "Name", unique: true);

        migrationBuilder.CreateTable(
            name: "AuditEntries",
            columns: table => new
            {
                Id = table.Column<Guid>(type: "TEXT", nullable: false),
                At = table.Column<DateTimeOffset>(type: "TEXT", nullable: false),
                ActorKind = table.Column<string>(type: "TEXT", nullable: false),
                ActorId = table.Column<string>(type: "TEXT", nullable: false),
                ActorName = table.Column<string>(type: "TEXT", nullable: false),
                Action = table.Column<string>(type: "TEXT", nullable: false),
                Target = table.Column<string>(type: "TEXT", nullable: false),
                Result = table.Column<string>(type: "TEXT", nullable: false),
                Detail = table.Column<string>(type: "TEXT", nullable: false),
            },
            constraints: table => table.PrimaryKey("PK_AuditEntries", x => x.Id));

        migrationBuilder.CreateTable(
            name: "SetupStates",
            columns: table => new
            {
                Id = table.Column<int>(type: "INTEGER", nullable: false),
                Status = table.Column<string>(type: "TEXT", nullable: false),
                SetupCodeHash = table.Column<byte[]>(type: "BLOB", nullable: true),
                SetupCodeIssuedAt = table.Column<DateTimeOffset>(type: "TEXT", nullable: false),
                AttemptCounters = table.Column<string>(type: "TEXT", nullable: false),
            },
            constraints: table => table.PrimaryKey("PK_SetupStates", x => x.Id));

        migrationBuilder.InsertData("SetupStates", new[] { "Id", "Status", "SetupCodeIssuedAt", "AttemptCounters" }, new object[] { 1, "AwaitingSetup", DateTimeOffset.MinValue, "{}" });

        migrationBuilder.CreateTable(
            name: "Settings",
            columns: table => new
            {
                Id = table.Column<int>(type: "INTEGER", nullable: false),
                ListenAddresses = table.Column<string>(type: "TEXT", nullable: true),
            },
            constraints: table => table.PrimaryKey("PK_Settings", x => x.Id));

        migrationBuilder.InsertData("Settings", new[] { "Id" }, new object[] { 1 });
    }

    protected override void Down(MigrationBuilder migrationBuilder)
    {
        migrationBuilder.DropTable("Administrators");
        migrationBuilder.DropTable("Sessions");
        migrationBuilder.DropTable("ApiTokens");
        migrationBuilder.DropTable("AuditEntries");
        migrationBuilder.DropTable("SetupStates");
        migrationBuilder.DropTable("Settings");
    }
}
