// Package the verified Windows x64 release distribution into its Inno Setup installer and update package.
// 将已验证的 Windows x64 正式版发行物打包为 Inno Setup 安装程序与更新包。
import { spawn } from 'node:child_process';
import { access, mkdir, readdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { buildInputPaths } from './build-input-paths.mjs';
import { managedRuntimePlatformKey } from './desktop-runtime.mjs';
import { obtainLockedInput } from './locked-download.mjs';
import { inspectReleaseDistribution } from './release-package.mjs';

// The repository is resolved from this script rather than the caller's working directory.
// 仓库由脚本位置确定，不依赖调用方工作目录。
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// installerAppId identifies every Vulcan Code installation for upgrades and uninstall; never change it.
// installerAppId 标识每一次 Vulcan Code 安装，用于升级与卸载；不得修改。
const installerAppId = '4AA6A407-B39F-4AF7-9F9B-167D14D64368';

// installerAssetName names the setup program as the release manifest's `inno` download must
// (`DownloadKind::Inno` in vulcan-install-core); the compiler appends `.exe`.
// installerAssetName 按版本清单 `inno` 下载项要求的名称（vulcan-install-core 的 `DownloadKind::Inno`）
// 为安装程序命名；编译器会追加 `.exe`。
export function installerAssetName(layout, version) {
  return `VulcanCode-${version}-${managedRuntimePlatformKey(layout)}-setup`;
}

// updateAssetName names the update package as the release manifest's `innoUpdate` must
// (`InnoUpdate::file_name` in vulcan-install-core); the compiler appends `.exe`.
// updateAssetName 按版本清单 `innoUpdate` 要求的名称（vulcan-install-core 的 `InnoUpdate::file_name`）
// 为更新包命名；编译器会追加 `.exe`。
export function updateAssetName(layout, version) {
  return `VulcanCode-${version}-${managedRuntimePlatformKey(layout)}-update`;
}

// installRecordFile is the install record every complete version directory holds (`RECORD_FILE_NAME` in
// vulcan-install-core): the release tool writes it into the tree, and an in-app assembly writes it last.
// installRecordFile 是每个完整版本目录都有的安装记录（vulcan-install-core 的 `RECORD_FILE_NAME`）：发布工具
// 将其写入发行树，应用内组装在最后写入它。
export const installRecordFile = '.vulcan-install.json';

// innoSignToolName names the sign tool a signed build passes to the compiler with `/S<name>=<command>`;
// Inno Setup then signs the setup program and the uninstaller it embeds.
// innoSignToolName 是签名构建通过 `/S<名称>=<命令>` 传给编译器的签名工具名；Inno Setup 随后为安装程序及
// 其内嵌的卸载程序签名。
export const innoSignToolName = 'vulcan';

// The multi-version layout below the install directory (plan section 4); `vulcan-install-core`
// (`install_root.rs`) and the launcher (`apps/vulcan-code-launcher`) read the same names.
// 安装目录下的多版本布局（方案第四节）；`vulcan-install-core`（`install_root.rs`）与启动器
// （`apps/vulcan-code-launcher`）读取相同的名称。
export const installLayout = Object.freeze({
  // launcher is the stable desktop entry every shortcut and taskbar pin points at.
  // launcher 是所有快捷方式与任务栏固定项指向的稳定桌面入口。
  launcher: 'VulcanCode.exe',
  // cli holds only the PATH forwarder, so adding it to PATH exposes nothing else.
  // cli 只包含 PATH 转发器，因此加入 PATH 不会暴露其他程序。
  cli: 'cli',
  forwarder: 'vulcan-tui.exe',
  versions: 'versions',
  updates: 'updates',
  current: 'current.json',
  // setup holds the setup program (`apps/vulcan-code-setup`, `SETUP_DIR` and `SETUP_PROGRAM`), released on
  // its own and bundled as published, so `vulcan-code-setup status` and `uninstall` work here too.
  // setup 存放独立发布的安装器（`apps/vulcan-code-setup` 的 `SETUP_DIR` 与 `SETUP_PROGRAM`），按发布原样内置，
  // 使 `vulcan-code-setup status` 与 `uninstall` 在此同样可用。
  setup: 'setup',
  setupProgram: 'vulcan-code-setup.exe',
});

// launcherSources maps each installed launcher program to the shipped binary it is copied from.
// launcherSources 将每个已安装的启动程序映射到其复制来源的分发二进制。
const launcherSources = Object.freeze([
  { binary: 'vulcan-code-launcher', destination: installLayout.launcher },
  { binary: 'vulcan-tui-forwarder', destination: `${installLayout.cli}\\${installLayout.forwarder}` },
]);

// releaseVersionPattern accepts the version forms release archives use; it also names a directory.
// releaseVersionPattern 接受发行归档使用的版本形式；它同时用作目录名。
const releaseVersionPattern = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?$/;

// issValue rejects characters that would end an Inno Setup quoted value, so paths cannot inject directives.
// issValue 拒绝会结束 Inno Setup 引号值的字符，避免路径注入指令。
function issValue(value) {
  if (/["\r\n]/.test(value)) throw new Error(`Value cannot be written into an Inno Setup script: ${value}`);
  return value;
}

// pascalString quotes a value for the [Code] section; doubling the quote is Pascal's only escape.
// pascalString 为 [Code] 节引用一个值；Pascal 唯一的转义是将引号加倍。
function pascalString(value) {
  if (/[\r\n]/.test(value)) throw new Error(`Value cannot be written into Pascal code: ${value}`);
  return `'${value.replaceAll("'", "''")}'`;
}

// desktopShortcutIdentity checks the release notification identity the desktop process applies to itself.
// desktopShortcutIdentity 检查桌面进程为自身设置的正式版通知身份。
// Shortcuts carry the same AppUserModelID, so the taskbar groups the window with them and pins the launcher.
// 快捷方式携带相同的 AppUserModelID，使任务栏将窗口与其归为一组，并固定启动器。
function desktopShortcutIdentity(identity) {
  const { name, appUserModelId, toastActivatorClsid } = identity ?? {};
  if (typeof name !== 'string' || !/^[A-Za-z0-9 ]+$/.test(name)) throw new Error(`Invalid desktop application name: ${name}.`);
  if (typeof appUserModelId !== 'string' || !/^[A-Za-z0-9.]+$/.test(appUserModelId) || appUserModelId.includes('.debug.')) {
    throw new Error(`The desktop executable has no release AppUserModelID: ${appUserModelId}.`);
  }
  const clsid = /^\{([0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12})\}$/i.exec(toastActivatorClsid ?? '');
  if (!clsid) throw new Error(`Invalid toast activator CLSID: ${toastActivatorClsid}.`);
  return { name, appUserModelId, clsid: clsid[1].toUpperCase() };
}

// innoSetupScript renders the installer definition for one verified distribution.
// innoSetupScript 为一份已验证发行物生成安装程序定义。
// The distribution is installed as versions\<version>; the launcher, the PATH forwarder and current.json sit
// beside it. entries are the distribution's top-level names, which are also the top-level release files of
// the former single-directory layout, so installing over that layout removes exactly those, never user data.
// identity is the desktop executable's release notification identity. setupProgram, when given, is the verified
// stable setup program to bundle. signed adds the sign tool directive.
// update builds the update package instead: the same AppId, sections and [Code], without the version's files. An
// in-app update across install epochs assembles versions\<version> by digest first and runs this package to switch
// to it, so Inno still owns the launchers, shortcuts and uninstall entry. It refuses a root where that version is
// not complete, and never moves that version aside or removes it.
// 发行物安装为 versions\<版本>；启动器、PATH 转发器与 current.json 位于其旁。entries 是发行物的顶层名称，
// 同时也是旧单目录布局的顶层发行文件，因此覆盖安装旧布局时只删除这些条目，绝不删除用户数据。identity 是
// 桌面可执行文件的正式版通知身份。若给出 setupProgram，它是要内置的已校验 stable 安装器。signed 加入签名工具指令。
// update 改为生成更新包：AppId、各节与 [Code] 相同，但不含版本文件。跨安装纪元的应用内更新先按摘要组装
// versions\<版本>，再运行此包切换到该版本，使启动器、快捷方式与卸载项仍归 Inno 管理。该版本不完整的根目录会被
// 拒绝，且绝不将该版本移开或删除。
export function innoSetupScript({ root, layout, version, entries, outputDir, icon, identity, setupProgram = null, signed = false, update = false }) {
  if (layout.platform !== 'win32' || layout.architecture !== 'x64') {
    throw new Error(`Only a Windows x64 distribution can be packaged; found ${layout.platform}-${layout.architecture}.`);
  }
  if (!releaseVersionPattern.test(version)) throw new Error(`Invalid release version: ${version}.`);
  if (entries.length === 0) throw new Error(`The distribution in ${root} is empty.`);
  // A distribution entry with a layout name would be deleted by the legacy cleanup of every later install.
  // 与布局名称相同的发行条目会在之后每次安装的旧布局清理中被删除。
  const reserved = new Set(Object.values(installLayout).map(name => name.toLowerCase()));
  const clash = entries.find(entry => reserved.has(entry.toLowerCase()) || /^unins\d+\./i.test(entry));
  if (clash) throw new Error(`The distribution entry ${clash} collides with the install layout.`);
  const { name: desktopName, appUserModelId, clsid } = desktopShortcutIdentity(identity);
  const { launcher, cli, forwarder, versions, updates, current, setup } = installLayout;
  const versionDir = `${versions}\\${issValue(version)}`;
  // Former-layout names are removed only where that layout is proven present (FormerLayoutPresent), so an
  // install into an unrelated non-empty directory never deletes a user's bin, config or tools.
  // 只有确认存在旧布局（FormerLayoutPresent）时才删除旧布局名称，安装到无关的非空目录时绝不删除用户的
  // bin、config 或 tools。
  const legacy = entries
    .map(entry => `Type: filesandordirs; Name: "{app}\\${issValue(entry)}"; Check: FormerLayoutPresent`)
    .join('\n');
  const launcherFiles = launcherSources.map(({ binary, destination }) => {
    const slash = destination.lastIndexOf('\\');
    const directory = slash === -1 ? '{app}' : `{app}\\${destination.slice(0, slash)}`;
    return `Source: "${issValue(root)}\\bin\\${binary}.exe"; DestDir: "${directory}"; DestName: "${destination.slice(slash + 1)}"; Flags: ignoreversion`;
  }).join('\n');
  const setupFile = setupProgram === null
    ? ''
    : `\nSource: "${issValue(setupProgram)}"; DestDir: "{app}\\${setup}"; DestName: "${installLayout.setupProgram}"; Flags: ignoreversion`;
  // The update package installs no version files; the version it switches to was assembled by the app.
  // 更新包不安装版本文件；它切换到的版本由应用组装。
  const versionFiles = update
    ? ''
    : `Source: "${issValue(root)}\\*"; DestDir: "{app}\\${versionDir}"; Flags: ignoreversion recursesubdirs createallsubdirs\n`;
  // The update package refuses a root without the complete version, before anything is renamed; the full
  // installer instead moves a same-version directory aside, so a reinstall starts from a clean one.
  // 更新包在任何改名之前拒绝缺少完整版本的根目录；完整安装包则将同版本目录移开，使重新安装从干净目录开始。
  const versionCheck = update
    ? `  // The update package carries no version files; it only finishes a version the app assembled.
  if not FileExists(VersionDir + '\\${installRecordFile}') then
  begin
    Result := 'Vulcan Code ' + InstalledVersion + ' is not prepared in ' + ExpandConstant('{app}')
      + '. This package only finishes an update started from Vulcan Code; use the full installer instead.';
    Exit;
  end;
`
    : '';
  const repairAside = update
    ? ''
    : `  // A same-version reinstall starts from a clean directory; the old one is kept until the new one is complete.
  if DirExists(VersionDir) then
  begin
    RepairAside := ExpandConstant('{app}\\${updates}\\') + InstalledVersion + '.reinstalling-'
      + GetDateTimeString('yyyymmddhhnnsszzz', #0, #0);
    ForceDirectories(ExpandConstant('{app}\\${updates}'));
    if not RenameFile(VersionDir, RepairAside) then
    begin
      Log('Version ' + InstalledVersion + ' could not be moved aside; its files are overwritten in place.');
      RepairAside := '';
    end;
  end;
`;
  const shortcut = `Filename: "{app}\\${launcher}"; AppUserModelID: "${appUserModelId}"; AppUserModelToastActivatorCLSID: "${clsid}"`;
  const currentJson = JSON.stringify({ version, path: `${versions}\\${version}` });
  return `; Generated by scripts/windows-installer.mjs; do not edit.
[Setup]
AppId={{${installerAppId}}
AppName=Vulcan Code
AppVersion=${issValue(version)}
AppVerName=Vulcan Code ${issValue(version)}
AppPublisher=OpenVulcan
DefaultDirName={autopf}\\VulcanCode
DefaultGroupName=Vulcan Code
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/max
SolidCompression=yes
LZMAUseSeparateProcess=yes
CloseApplications=yes
ChangesEnvironment=yes
WizardStyle=modern
SetupIconFile=${issValue(icon)}
UninstallDisplayIcon={app}\\${launcher}
OutputDir=${issValue(outputDir)}
OutputBaseFilename=${issValue((update ? updateAssetName : installerAssetName)(layout, version))}
${signed ? `SignTool=${innoSignToolName}\n` : ''}
[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "addtopath"; Description: "Add vulcan-tui to PATH for the current user"; GroupDescription: "Command line:"

[InstallDelete]
; Top-level release files of the former single-directory layout. A same-version reinstall and launchers
; renamed aside are handled in [Code], where an unfinished install can still restore them.
${legacy}

[Files]
${versionFiles}${launcherFiles}${setupFile}

[Icons]
Name: "{autoprograms}\\Vulcan Code"; ${shortcut}
Name: "{autodesktop}\\Vulcan Code"; ${shortcut}; Tasks: desktopicon

[Registry]
; Electron registers the toast activator for the running desktop executable; remove it with the program.
Root: HKCU; Subkey: "Software\\Classes\\CLSID\\{{${clsid}}"; Flags: uninsdeletekey dontcreatekey

[Run]
Filename: "{app}\\${launcher}"; Description: "{cm:LaunchProgram,Vulcan Code}"; Flags: nowait postinstall skipifsilent
; An update started from the app (vulcan-app-update INSTALLER_ARGUMENTS) runs silently and asks for a relaunch.
Filename: "{app}\\${launcher}"; Flags: nowait skipifnotsilent; Check: RelaunchRequested

[UninstallDelete]
Type: filesandordirs; Name: "{app}\\${versions}"
Type: filesandordirs; Name: "{app}\\${updates}"
Type: filesandordirs; Name: "{app}\\${cli}"
Type: filesandordirs; Name: "{app}\\${setup}"
Type: files; Name: "{app}\\${current}"
Type: files; Name: "{app}\\${current}.partial"
Type: files; Name: "{app}\\${launcher}.*.old"
Type: files; Name: "{app}\\${cli}\\${forwarder}.*.old"
; Electron keeps its own Start menu shortcut pointing at the running desktop executable.
Type: files; Name: "{userprograms}\\${desktopName}.lnk"
${legacy}

[Code]
const
  MOVEFILE_REPLACE_EXISTING = 1;
  MOVEFILE_WRITE_THROUGH = 8;
  // TFileStream mode fmOpenWrite or fmShareDenyNone.
  OPEN_WRITE_SHARE_ALL = $41;
  // Exit code of a run that installed the files but could not switch to them; Setup itself uses 1-8.
  SWITCH_FAILED_EXIT_CODE = 9;
  CurrentJson = ${pascalString(currentJson)};
  InstalledVersion = ${pascalString(version)};

var
  // Launchers renamed by PrepareToInstall, restored when the installation does not finish.
  MovedAside: TArrayOfString;
  // The installed version's directory moved aside for a same-version reinstall; restored when the
  // installation does not finish, deleted when it does.
  RepairAside: String;
  // Whether {app} holds the former single-directory layout, decided before anything is removed.
  FormerLayout: Boolean;
  Installed: Boolean;
  // Whether current.json could not be pointed at the installed version; the run then ends with
  // SWITCH_FAILED_EXIT_CODE.
  SwitchFailed: Boolean;

function MoveFileEx(ExistingFileName, NewFileName: String; Flags: Cardinal): Boolean;
  external 'MoveFileExW@kernel32.dll stdcall';

function CliDir: String;
begin
  Result := ExpandConstant('{app}\\${cli}');
end;

function VersionDir: String;
begin
  Result := ExpandConstant('{app}\\${versionDir}');
end;

// Check of the former-layout entries in [InstallDelete] and [UninstallDelete]; Setup evaluates both
// while installing, after PrepareToInstall has decided FormerLayout.
function FormerLayoutPresent: Boolean;
begin
  Result := FormerLayout;
end;

// Deletes the files matching Pattern in Dir; a file still in use stays for a later install.
procedure DeleteMatchingFiles(Dir, Pattern: String);
var
  Search: TFindRec;
begin
  if not FindFirst(Dir + '\\' + Pattern, Search) then Exit;
  try
    repeat
      if Search.Attributes and FILE_ATTRIBUTE_DIRECTORY = 0 then DeleteFile(Dir + '\\' + Search.Name);
    until not FindNext(Search);
  finally
    FindClose(Search);
  end;
end;

// Deletes the directories matching Pattern in Dir; one still in use stays for a later install.
procedure DeleteMatchingDirs(Dir, Pattern: String);
var
  Search: TFindRec;
begin
  if not FindFirst(Dir + '\\' + Pattern, Search) then Exit;
  try
    repeat
      if Search.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0 then DelTree(Dir + '\\' + Search.Name, True, True, True);
    until not FindNext(Search);
  finally
    FindClose(Search);
  end;
end;

// A running launcher cannot be overwritten but can be renamed, so a TUI left open in a terminal never
// blocks an install; the renamed copy is deleted by a later install or by uninstall.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Targets: TArrayOfString;
  Aside: String;
  I: Integer;
begin
  Result := '';
${versionCheck}  // Only a directory holding both of these was installed by the former single-directory layout.
  FormerLayout := FileExists(ExpandConstant('{app}\\runtime-layout.json'))
    and FileExists(ExpandConstant('{app}\\bin\\vulcan-code-service.exe'));
  // Leftovers of earlier installs go first, so nothing this run moves aside is deleted before it finishes.
  DeleteMatchingFiles(ExpandConstant('{app}'), '${launcher}.*.old');
  DeleteMatchingFiles(CliDir, '${forwarder}.*.old');
  DeleteMatchingDirs(ExpandConstant('{app}\\${updates}'), '*.reinstalling-*');
${repairAside}  SetArrayLength(Targets, 2);
  Targets[0] := ExpandConstant('{app}\\${launcher}');
  Targets[1] := CliDir + '\\${forwarder}';
  for I := 0 to GetArrayLength(Targets) - 1 do
    if FileExists(Targets[I]) then
    begin
      Aside := Targets[I] + '.' + GetDateTimeString('yyyymmddhhnnsszzz', #0, #0) + '.old';
      if RenameFile(Targets[I], Aside) then
      begin
        SetArrayLength(MovedAside, GetArrayLength(MovedAside) + 2);
        MovedAside[GetArrayLength(MovedAside) - 2] := Targets[I];
        MovedAside[GetArrayLength(MovedAside) - 1] := Aside;
      end;
    end;
end;

// Splits a PATH value on semicolons and drops every entry naming Dir, keeping all other entries,
// empty ones included, exactly as they were.
function PathWithout(Path, Dir: String; var Found: Boolean): String;
var
  Rest, Entry: String;
  P: Integer;
  First: Boolean;
begin
  Result := '';
  Found := False;
  First := True;
  Rest := Path;
  repeat
    P := Pos(';', Rest);
    if P = 0 then
      Entry := Rest
    else
    begin
      Entry := Copy(Rest, 1, P - 1);
      Rest := Copy(Rest, P + 1, Length(Rest));
    end;
    if CompareText(RemoveBackslashUnlessRoot(Trim(Entry)), RemoveBackslashUnlessRoot(Dir)) = 0 then
      Found := True
    else
    begin
      if not First then Result := Result + ';';
      Result := Result + Entry;
      First := False;
    end;
  until P = 0;
end;

// Writes the user PATH. An existing value keeps its registry type (RegWriteStringValue preserves
// REG_EXPAND_SZ and leaves REG_SZ as REG_SZ), so %VARIABLE% entries the user already has are not
// expanded where they were not before; only a new value is created as REG_EXPAND_SZ.
function WriteUserPath(Value: String; Existed: Boolean): Boolean;
begin
  if Existed then
    Result := RegWriteStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Value)
  else
    Result := RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Value);
end;

// Adds or removes {app}\\${cli} in the user PATH (HKCU\\Environment), touching no other entry.
// ChangesEnvironment broadcasts WM_SETTINGCHANGE when Setup or Uninstall finishes.
procedure UpdateUserPath(Add: Boolean);
var
  Path, Remaining: String;
  Existed, Found: Boolean;
begin
  Existed := RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path);
  if not Existed then Path := '';
  Remaining := PathWithout(Path, CliDir, Found);
  if Add then
  begin
    if Found then Exit;
    // A trailing separator stays trailing, so removing the entry restores the value byte for byte.
    if Path = '' then
      Path := CliDir
    else if Path[Length(Path)] = ';' then
      Path := Path + CliDir + ';'
    else
      Path := Path + ';' + CliDir;
    if not WriteUserPath(Path, Existed) then
      SuppressibleMsgBox('Could not add ' + CliDir + ' to the user PATH.', mbError, MB_OK, IDOK);
  end
  else if Found then
  begin
    if Remaining = '' then
      RegDeleteValue(HKEY_CURRENT_USER, 'Environment', 'Path')
    else if not WriteUserPath(Remaining, Existed) then
      Log('Could not remove ' + CliDir + ' from the user PATH.');
  end;
end;

// True when the app started this run and asked to be started again (/RELAUNCH=1, mirrored by
// INSTALLER_ARGUMENTS in crates/vulcan-app-update/src/lib.rs).
function RelaunchRequested: Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

// Reads the "version" string of current.json; the writers (this installer and vulcan-install-core)
// emit compact JSON whose version needs no escapes.
function CurrentVersion: String;
var
  Text: AnsiString;
  S: String;
  P: Integer;
begin
  Result := '';
  if not LoadStringFromFile(ExpandConstant('{app}\\${current}'), Text) then Exit;
  S := String(Text);
  P := Pos('"version"', S);
  if P = 0 then Exit;
  S := Trim(Copy(S, P + 9, Length(S)));
  if (S = '') or (S[1] <> ':') then Exit;
  S := Trim(Copy(S, 2, Length(S)));
  if (S = '') or (S[1] <> '"') then Exit;
  S := Copy(S, 2, Length(S));
  P := Pos('"', S);
  if P > 0 then Result := Copy(S, 1, P - 1);
end;

// Replaces current.json atomically: a complete partial file is renamed over the old one. Returns
// False with the reason logged and shown; an exception here would only be logged and Setup would
// still exit with 0.
function SwitchCurrentVersion: Boolean;
var
  Target, Problem: String;
begin
  Result := False;
  Target := ExpandConstant('{app}\\${current}');
  if not SaveStringToFile(Target + '.partial', CurrentJson, False) then
    Problem := 'Could not write ' + Target + '.partial.'
  else if not MoveFileEx(Target + '.partial', Target, MOVEFILE_REPLACE_EXISTING or MOVEFILE_WRITE_THROUGH) then
    Problem := 'Could not replace ' + Target + '.'
  else
  begin
    Result := True;
    Exit;
  end;
  Log(Problem);
  SuppressibleMsgBox(Problem + ' The previous version stays in use.', mbCriticalError, MB_OK, IDOK);
end;

// A run whose version switch failed reports it to the caller, such as the in-app update.
function GetCustomSetupExitCode: Integer;
begin
  if SwitchFailed then Result := SWITCH_FAILED_EXIT_CODE else Result := 0;
end;

// True when a program or library under Dir is loaded by a running process: opening a running image
// for writing fails even when every kind of sharing is allowed, and the open changes nothing.
// Windows lets a directory holding a running executable be renamed, so a rename cannot tell.
function ImagesInUse(Dir: String): Boolean;
var
  Search: TFindRec;
  Path, Ext: String;
  Stream: TFileStream;
begin
  Result := False;
  if not FindFirst(Dir + '\\*', Search) then Exit;
  try
    repeat
      if (Search.Name <> '.') and (Search.Name <> '..') then
      begin
        Path := Dir + '\\' + Search.Name;
        if Search.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0 then
        begin
          if Search.Attributes and FILE_ATTRIBUTE_REPARSE_POINT = 0 then Result := ImagesInUse(Path);
        end
        else
        begin
          Ext := LowerCase(ExtractFileExt(Search.Name));
          if (Ext = '.exe') or (Ext = '.dll') then
            try
              Stream := TFileStream.Create(Path, OPEN_WRITE_SHARE_ALL);
              Stream.Free;
            except
              Result := True;
            end;
        end;
      end;
    until Result or not FindNext(Search);
  finally
    FindClose(Search);
  end;
end;

// Keeps the installed version and the one it replaces. A version with a running program stays; each
// other one is renamed into updates\\ first, so the launcher never sees a half-deleted version.
procedure RemoveOlderVersions(Previous: String);
var
  Search: TFindRec;
  Versions, Updates, Name: String;
begin
  Versions := ExpandConstant('{app}\\${versions}');
  Updates := ExpandConstant('{app}\\${updates}');
  if not FindFirst(Versions + '\\*', Search) then Exit;
  try
    repeat
      Name := Search.Name;
      if (Name <> '.') and (Name <> '..') and (Search.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0)
        and (CompareText(Name, InstalledVersion) <> 0) and (CompareText(Name, Previous) <> 0) then
      begin
        if ImagesInUse(Versions + '\\' + Name) then
          Log('Version ' + Name + ' is in use and stays installed.')
        else
        begin
          ForceDirectories(Updates);
          if RenameFile(Versions + '\\' + Name, Updates + '\\' + Name + '.removing') then
            DelTree(Updates + '\\' + Name + '.removing', True, True, True)
          else
            Log('Version ' + Name + ' could not be moved and stays installed.');
        end;
      end;
    until not FindNext(Search);
  finally
    FindClose(Search);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Previous: String;
  I: Integer;
begin
  if CurStep = ssPostInstall then
  begin
    Previous := CurrentVersion;
    // The install record ${installRecordFile} is in the version directory: the release tool wrote it into the
    // tree before compiling, and the update package runs only where the app's assembly wrote it, so
    // current.json only ever points at a recorded version.
    if not SwitchCurrentVersion then
    begin
      // The copied files stay and nothing is cleaned up: the still current version keeps working,
      // a program relaunched by /RELAUNCH may already run from them, and a repeated run switches.
      SwitchFailed := True;
      Installed := True;
      Exit;
    end;
    Installed := True;
    // The replaced copies are no longer needed; any still in use are removed by a later install.
    for I := 0 to GetArrayLength(MovedAside) div 2 - 1 do
      DeleteFile(MovedAside[I * 2 + 1]);
    if RepairAside <> '' then DelTree(RepairAside, True, True, True);
    // Repairing the current version does not know which version it replaced, so it keeps them all.
    if CompareText(Previous, InstalledVersion) <> 0 then RemoveOlderVersions(Previous);
    UpdateUserPath(WizardIsTaskSelected('addtopath'));
  end;
end;

procedure DeinitializeSetup;
var
  I: Integer;
begin
  if Installed then Exit;
  for I := 0 to GetArrayLength(MovedAside) div 2 - 1 do
    if not FileExists(MovedAside[I * 2]) then
      RenameFile(MovedAside[I * 2 + 1], MovedAside[I * 2]);
  // A partly copied version directory gives way to the complete one it replaced.
  if RepairAside <> '' then
  begin
    if DirExists(VersionDir) then DelTree(VersionDir, True, True, True);
    if not RenameFile(RepairAside, VersionDir) then
      Log('Version ' + InstalledVersion + ' could not be restored from ' + RepairAside + '.');
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then UpdateUserPath(False);
end;
`;
}

// runInherited runs one tool with the console attached, so long compressions show their progress.
// runInherited 在附加控制台的情况下运行工具，使长时间压缩能显示进度。
function runInherited(executable, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, { stdio: 'inherit', windowsHide: true });
    child.on('error', reject);
    child.on('exit', code => (code === 0 ? resolve() : reject(new Error(`${path.basename(executable)} exited with code ${code}.`))));
  });
}

// ensureInnoSetup installs the locked Inno Setup compiler into third_party and returns ISCC.exe.
// ensureInnoSetup 将锁定版本的 Inno Setup 编译器安装到 third_party，并返回 ISCC.exe。
async function ensureInnoSetup() {
  const lock = JSON.parse(await readFile(new URL('./build-inputs.lock.json', import.meta.url), 'utf8'));
  const { version, installer } = lock.inno_setup;
  const paths = buildInputPaths(repository);
  const home = path.join(paths.innoSetup, version);
  const compiler = path.join(home, 'ISCC.exe');
  try {
    await access(compiler);
    return compiler;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  // The installer runs from its own directory with its expected name; the cache entry stays digest-named.
  // 安装器以预期名称在独立目录中运行；缓存条目仍按摘要命名。
  const download = path.join(paths.innoSetup, '.download');
  const partial = `${home}.partial`;
  await rm(download, { recursive: true, force: true });
  await rm(partial, { recursive: true, force: true });
  await mkdir(download, { recursive: true });
  try {
    const executable = path.join(download, installer.name);
    await writeFile(executable, await obtainLockedInput(installer, paths.lockedDownloads));
    console.log(`==> Installing Inno Setup ${version} into ${home}`);
    // Portable mode writes no uninstall entry; the current-user mode never asks for elevation.
    // 便携模式不写入卸载项；当前用户模式不请求提权。
    await runInherited(executable, ['/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', '/PORTABLE=1', `/DIR=${partial}`]);
    await access(path.join(partial, 'ISCC.exe'));
    await rename(partial, home);
  } finally {
    await rm(download, { recursive: true, force: true });
    await rm(partial, { recursive: true, force: true });
  }
  return compiler;
}

// packageWindowsInstaller verifies the release distribution, then compiles the setup program from it as is.
// signCommand, when given, is the command Inno Setup runs for each file it signs, with `$f` for the file.
// packageWindowsInstaller 验证正式版发行物，然后按其现状编译安装程序。若给出 signCommand，Inno Setup 对每个
// 待签名文件运行该命令，`$f` 代表文件。setupProgram 若给出，是调用方已校验的 stable 安装器
// （`vulcan-release-tool setup-check`），按原样内置、不再签名；不给出时安装程序不内置安装器。
async function packageWindowsInstaller(outputOverride, signCommand, setupProgram) {
  if (process.platform !== 'win32' || process.arch !== 'x64') throw new Error('The Windows installer is built on Windows x64 only.');
  const { root, layout, version } = await inspectReleaseDistribution();
  const outputDir = outputOverride ?? path.join(repository, 'target', 'release-packages', version);
  await mkdir(outputDir, { recursive: true });
  // The launchers are copied out of the version tree, so a distribution built without them cannot be installed.
  // 启动器从版本目录中复制出来，因此缺少启动器的发行物无法安装。
  for (const { binary } of launcherSources) await access(path.join(root, 'bin', `${binary}.exe`));
  if (setupProgram) await access(setupProgram);
  else console.log('==> No --setup given; the installer bundles no setup program.');
  // The shortcuts take their identity from the same resolver the packaged desktop process uses.
  // 快捷方式的身份取自打包后桌面进程所用的同一解析器。
  const { resolveGuiNotificationIdentity } = await import(pathToFileURL(
    path.join(repository, 'apps', 'vulcan-code-desktop', 'dist', 'gui', 'notification-identity-values.js'),
  ).href);
  const compiler = await ensureInnoSetup();
  const common = {
    root, layout, version, outputDir,
    entries: (await readdir(root)).sort(),
    icon: path.join(repository, 'apps', 'vulcan-code-service', 'icons', 'icon.ico'),
    identity: resolveGuiNotificationIdentity(path.join(root, layout.desktopExecutable), 'win32'),
    setupProgram: setupProgram ?? null,
    signed: Boolean(signCommand),
  };
  // The full installer and the update package come from one template and are signed by the same command.
  // 完整安装包与更新包出自同一模板，并由同一命令签名。
  for (const [update, name] of [[false, installerAssetName(layout, version)], [true, updateAssetName(layout, version)]]) {
    // The generated script lives beside the setup program so a failed build can be inspected.
    // 生成的脚本与安装程序放在一起，便于检查失败的构建。
    const scriptPath = path.join(outputDir, `${name}.iss`);
    await writeFile(scriptPath, innoSetupScript({ ...common, update }));
    console.log(`==> Compiling ${path.basename(scriptPath)} (${signCommand ? 'signed' : 'unsigned'})`);
    await runInherited(compiler, ['/Qp', ...(signCommand ? [`/S${innoSignToolName}=${signCommand}`] : []), scriptPath]);
    console.log(`Windows ${update ? 'update package' : 'installer'}: ${path.join(outputDir, `${name}.exe`)}`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...rest] = process.argv.slice(2);
  // options holds each `--name value` pair; a repeated, unknown or valueless flag is rejected.
  // options 保存每个 `--名称 值` 对；重复、未知或缺少值的参数一律拒绝。
  const options = new Map();
  for (let index = 0; index < rest.length; index += 2) {
    const [flag, value] = [rest[index], rest[index + 1]];
    if (!['--out', '--sign', '--setup'].includes(flag) || value === undefined || options.has(flag)) options.set('invalid', true);
    else options.set(flag, value);
  }
  if (command === 'package' && !options.has('invalid')) {
    await packageWindowsInstaller(
      options.has('--out') ? path.resolve(options.get('--out')) : undefined,
      options.get('--sign'),
      options.has('--setup') ? path.resolve(options.get('--setup')) : undefined,
    );
  } else {
    throw new Error('Usage: node scripts/windows-installer.mjs package [--out <dir>] [--sign <command with $f>] [--setup <file>]');
  }
}
