using System.Text;
using System.IO;
using System.Text.Json.Serialization;
using System.Text.Json;
using System.Security.Cryptography;
using BgiBridge.Catalog;
using BgiBridge.Protocol;
class RecoveryConfig {public bool Enabled {get;set;}=true;public int Other {get;set;}=7;public string ApiKey {get;set;}="secret-fixture";[JsonIgnore] public Action? OnAnyChangedAction {get;set;}}
class RecoveryChecks {
 static readonly JsonSerializerOptions Json=new(){PropertyNamingPolicy=JsonNamingPolicy.CamelCase,WriteIndented=true};
 static void Check(bool test,string text){if(!test)throw new Exception(text);}
 static JsonElement Wire(object value)=>JsonSerializer.SerializeToElement(value);
 static string Version(string file)=>Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(file)));
 static string RecordVersion(string records,string id)=>Version(Path.Combine(records,$"config-change-{id}.json"));
 static void Reject(Action action,string code){try{action();}catch(BridgeException error)when(error.Code==code){return;}throw new Exception("Expected rejection "+code);}
 static void Main(string[] args){
  if(args.Length==2&&args[0]=="--inspect") {
   var list=Wire(SettingsRecovery.List(args[1]));
   var rows=list.GetProperty("records").EnumerateArray().ToArray();
   var changes=rows.Where(row=>row.GetProperty("paths").GetArrayLength()>0).ToArray();
   Check(changes.All(row=>row.GetProperty("fields").EnumerateArray().All(field=>field.GetProperty("label").GetString()!=field.GetProperty("path").GetString())),"Existing records lack readable labels");
   Console.WriteLine(JsonSerializer.Serialize(new{readOnlyInspection=true,records=rows.Length,settingChanges=changes.Length,completeSnapshots=rows.Count(row=>row.GetProperty("kind").GetString()=="snapshot"),unavailable=rows.Count(row=>row.GetProperty("kind").GetString()=="unavailable"),friendlyTitles=true,userConfigModified=false}));return;
  }
  var root=Path.Combine(Path.GetTempPath(),"sleepy-doll-recovery-validation");if(Directory.Exists(root))throw new Exception("Fixture directory exists; refusing to overwrite");
  Directory.CreateDirectory(root);Console.WriteLine(JsonSerializer.Serialize(new{fixture=root,cleanup="finally removes exact owned fixture"}));
  try{
   var user=Path.Combine(root,"User");var records=Path.Combine(root,"records");Directory.CreateDirectory(user);Directory.CreateDirectory(records);
   var executable=Path.Combine(root,"BetterGI.exe");File.WriteAllText(executable,"fixture");var file=Path.Combine(user,"config.json");
   var config=new RecoveryConfig();File.WriteAllText(file,"{\"enabled\":true,\"other\":7,\"apiKey\":\"secret-fixture\"}");
   var engine=new SettingsTransactionEngine(()=>config,()=>file,()=>Json,records,()=>executable);
   string Change(string path,object value){var entry=SettingsCatalog.Build(config).Single(entry=>entry.Path==path);var plan=Wire(engine.Preview([new(path,Wire(value),entry.ValueVersion)]));return Wire(engine.Commit(plan.GetProperty("planId").GetString()!)).GetProperty("changeId").GetString()!;}
   var original=Change("enabled",false);config.Other=18;File.WriteAllText(file,"{\"enabled\":false,\"other\":18,\"apiKey\":\"secret-fixture\"}");
   var preview=Wire(engine.PreviewRestore(original,RecordVersion(records,original),["enabled"]));
   config.Other=21;File.WriteAllText(file,"{\"enabled\":false,\"other\":21,\"apiKey\":\"secret-fixture\"}");
   var restored=Wire(engine.Commit(preview.GetProperty("planId").GetString()!));
   Check(config.Enabled&&config.Other==21,"Online restore changed unrelated live settings");Check(JsonDocument.Parse(File.ReadAllBytes(file)).RootElement.GetProperty("other").GetInt32()==21,"Online restore lost unrelated disk edits after preview");
   var restoreId=restored.GetProperty("changeId").GetString()!;var undo=Wire(engine.PreviewRestore(restoreId,RecordVersion(records,restoreId),["enabled"]));engine.Commit(undo.GetProperty("planId").GetString()!);Check(!config.Enabled&&config.Other==21,"Undo restore failed");
   var conflict=Wire(engine.PreviewRestore(original,RecordVersion(records,original),["enabled"]));config.Enabled=true;
   Reject(()=>engine.Commit(conflict.GetProperty("planId").GetString()!),"CONFIG_CONFLICT");config.Enabled=false;
   var offline=Wire(SettingsRecovery.Preview(records,original,RecordVersion(records,original),"fields",["enabled"],_=>false));
   var version=offline.GetProperty("currentVersion").GetString()!;Reject(()=>SettingsRecovery.Restore(records,original,RecordVersion(records,original),version,"fields",["enabled"],_=>true),"HOST_RUNNING");
   var partial=Wire(SettingsRecovery.Restore(records,original,RecordVersion(records,original),version,"fields",["enabled"],_=>false));
   var disk=JsonDocument.Parse(File.ReadAllBytes(file)).RootElement;Check(disk.GetProperty("enabled").GetBoolean()&&disk.GetProperty("other").GetInt32()==21,"Offline field recovery reset the whole file");
   var stale=Version(file);File.AppendAllText(file," ");Reject(()=>SettingsRecovery.Restore(records,original,RecordVersion(records,original),stale,"full",[],_=>false),"CONFIG_CONFLICT");
   File.WriteAllText(file,"broken-json");var emergency=Wire(SettingsRecovery.Preview(records,original,RecordVersion(records,original),"full",[],_=>false));Check(emergency.GetProperty("canApply").GetBoolean(),"Corrupt config cannot recover from valid backup");
   SettingsRecovery.Restore(records,original,RecordVersion(records,original),Version(file),"full",[],_=>false);Check(JsonDocument.Parse(File.ReadAllBytes(file)).RootElement.GetProperty("other").GetInt32()==7,"Whole recovery did not replace full backup");
   Check(SettingsRecovery.Label("genshinStartConfig.genshinStartArgs").Contains("启动"),"Legacy records still show raw field names");
   Check((string)SettingsRecovery.SafeValue("apiKey",Wire("never-display-this"))! == "已遮蔽的敏感值","Recovery leaked a sensitive value");
   var list=Wire(SettingsRecovery.List(records));Check(list.GetProperty("records").GetArrayLength()>=3,"Restore did not retain history and pre-restore backups");
   Console.WriteLine(JsonSerializer.Serialize(new{onlineFieldRecoveryPreservesUnrelatedChanges=true,undoRecovery=true,selectedFieldConflictRejected=true,offlinePartialRecovery=true,hostRunningRejected=true,staleFileRejected=true,corruptFileEmergencyRecovery=true,friendlyLegacyLabels=true,sensitiveValuesMasked=true,realUserDataTouched=false}));
  }finally {var expected=Path.GetFullPath(Path.Combine(Path.GetTempPath(),"sleepy-doll-recovery-validation"));if(Path.GetFullPath(root)!=expected)throw new Exception("Cleanup boundary mismatch");Directory.Delete(root,true);}
 }
}
