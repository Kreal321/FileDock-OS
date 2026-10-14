using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Migrations;
namespace FileDock.Management.Data.Migrations;

[DbContext(typeof(AppDbContext))]
[Migration("20261013120000_ConfigurationRecovery")]
public sealed class ConfigurationRecovery : Migration
{
    protected override void Up(MigrationBuilder migrationBuilder)
    {
        migrationBuilder.AddColumn<DateTimeOffset>("ConfigChangedAt", "Settings", "TEXT", nullable: true);
        migrationBuilder.AddColumn<long>("ConfigChangeSeq", "Settings", "INTEGER", nullable: false, defaultValue: 0L);
        migrationBuilder.AddColumn<string>("LastRestoreOperationId", "Settings", "TEXT", nullable: true);
        migrationBuilder.AddColumn<string>("DismissedRestoreOperationId", "Settings", "TEXT", nullable: true);
    }
    protected override void Down(MigrationBuilder migrationBuilder)
    {
        migrationBuilder.DropColumn("ConfigChangedAt", "Settings");
        migrationBuilder.DropColumn("ConfigChangeSeq", "Settings");
        migrationBuilder.DropColumn("LastRestoreOperationId", "Settings");
        migrationBuilder.DropColumn("DismissedRestoreOperationId", "Settings");
    }
}
