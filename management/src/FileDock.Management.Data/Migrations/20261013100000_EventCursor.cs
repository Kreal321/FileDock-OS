using Microsoft.EntityFrameworkCore.Infrastructure;
using Microsoft.EntityFrameworkCore.Migrations;
namespace FileDock.Management.Data.Migrations;

[DbContext(typeof(AppDbContext))]
[Migration("20261013100000_EventCursor")]
public sealed class EventCursor : Migration
{
    protected override void Up(MigrationBuilder migrationBuilder) => migrationBuilder.AddColumn<long>(name: "LastEventSeq", table: "Settings", type: "INTEGER", nullable: false, defaultValue: 0L);
    protected override void Down(MigrationBuilder migrationBuilder) => migrationBuilder.DropColumn(name: "LastEventSeq", table: "Settings");
}
