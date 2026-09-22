; Compile with scripts/build-installer.ps1 after building/testing both .8bi files.
#ifndef PluginVersion
  #error PluginVersion is required
#endif
#ifndef SourceRoot
  #error SourceRoot is required
#endif
#ifndef OutputRoot
  #error OutputRoot is required
#endif

[Setup]
#ifndef TestRoot
AppId=Seiza.Photoshop.Formats
DefaultDirName={autopf64}\Seiza\Photoshop Formats
PrivilegesRequired=admin
OutputBaseFilename=Seiza-Photoshop-Windows-x64-Setup-{#PluginVersion}
SetupMutex=SeizaPhotoshopFormatsInstaller
#endif
AppName=FITS and XISF for Photoshop
AppVersion={#PluginVersion}
AppPublisher=Seiza
AppPublisherURL=https://github.com/theatrus/xisf-photoshop
AppSupportURL=https://github.com/theatrus/xisf-photoshop/issues
DisableDirPage=yes
DisableWelcomePage=no
DisableProgramGroupPage=yes
UninstallDisplayName=FITS and XISF for Photoshop
ArchitecturesAllowed=x64compatible and not arm64
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern
OutputDir={#OutputRoot}
Compression=lzma2
SolidCompression=yes
SetupLogging=yes
CloseApplications=no
RestartApplications=no
RestartIfNeededByRun=no
UninstallLogMode=append
; A separate, non-shipping build exercises the same logic in an isolated tree.
#ifdef TestRoot
AppId=Seiza.Photoshop.Formats.InstallerTest
PrivilegesRequired=lowest
DefaultDirName={#TestRoot}\App
OutputBaseFilename=InstallerTest
SetupMutex=SeizaPhotoshopFormatsInstallerTest
UsePreviousAppDir=no
#endif

[Files]
Source: "{#SourceRoot}\dist\SeizaFITS.8bi"; DestDir: "{code:PluginDirectory}"; Flags: ignoreversion
Source: "{#SourceRoot}\dist\SeizaXISF.8bi"; DestDir: "{code:PluginDirectory}"; Flags: ignoreversion
Source: "{#SourceRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\NOTICE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Messages]
WelcomeLabel2=This will install FITS and XISF file support in Adobe's shared Photoshop plugin folder, for all installed Photoshop versions.%n%nClose Photoshop before continuing. Existing Seiza plugin copies found in standard or registered Photoshop folders will be backed up and disabled to avoid duplicates. Your plugin preferences are kept.
FinishedLabel=FITS and XISF support is installed. Start Photoshop, then use Help > About Plug-In > FITS or XISF to configure import settings.%n%nYou can remove these plugins later from Windows Settings > Apps. Preferences and backups are kept when uninstalling.

[Code]
var
  LegacyPaths, BackupPaths: TArrayOfString;
  BackupFolder: String;
  InstallSucceeded: Boolean;

function PluginDirectory(Param: String): String;
begin
#ifdef TestRoot
  Result := '{#TestRoot}\Shared\Seiza';
#else
  Result := ExpandConstant('{commoncf64}\Adobe\Plug-Ins\CC\Seiza');
#endif
end;

function BackupBase: String;
begin
#ifdef TestRoot
  Result := '{#TestRoot}\Backups';
#else
  Result := ExpandConstant('{commonappdata}\Seiza\Photoshop\InstallerBackups');
#endif
end;

function PhotoshopClosed: Boolean;
var Locator, Services, Processes: Variant;
begin
  Result := False;
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Services := Locator.ConnectServer('', 'root\cimv2');
#ifdef TestRoot
    Processes := Services.ExecQuery('SELECT ProcessId FROM Win32_Process WHERE Name = ''SeizaInstallerTestHost.exe''');
#else
    Processes := Services.ExecQuery('SELECT ProcessId FROM Win32_Process WHERE Name = ''Photoshop.exe''');
#endif
    Result := Processes.Count = 0;
  except
    Log('Could not check Photoshop processes: ' + GetExceptionMessage);
  end;
end;

procedure AddLegacy(Path: String);
var I, N: Integer;
begin
  Path := ExpandFileName(Path);
  if not FileExists(Path) then Exit;
  if CompareText(ExtractFileDir(Path), PluginDirectory('')) = 0 then Exit;
  for I := 0 to GetArrayLength(LegacyPaths) - 1 do
    if CompareText(LegacyPaths[I], Path) = 0 then Exit;
  N := GetArrayLength(LegacyPaths);
  SetArrayLength(LegacyPaths, N + 1);
  LegacyPaths[N] := Path;
end;

procedure ScanPlugins(Path: String);
begin
  Path := RemoveBackslashUnlessRoot(Path);
  AddLegacy(Path + '\SeizaFITS.8bi');
  AddLegacy(Path + '\SeizaXISF.8bi');
  AddLegacy(Path + '\Seiza\SeizaFITS.8bi');
  AddLegacy(Path + '\Seiza\SeizaXISF.8bi');
end;

procedure ScanAdobeFolder(Base: String);
var Found: TFindRec;
begin
  if FindFirst(Base + '\Adobe Photoshop*', Found) then
  try
    repeat
      if (Found.Attributes and FILE_ATTRIBUTE_DIRECTORY) <> 0 then
        ScanPlugins(Base + '\' + Found.Name + '\Plug-ins');
    until not FindNext(Found);
  finally
    FindClose(Found);
  end;
end;

procedure ScanRegistry(RootKey: Integer);
var Keys: TArrayOfString; I: Integer; Key, Path: String;
begin
  if RegGetSubkeyNames(RootKey, 'SOFTWARE\Adobe\Photoshop', Keys) then
    for I := 0 to GetArrayLength(Keys) - 1 do begin
      Key := 'SOFTWARE\Adobe\Photoshop\' + Keys[I];
      if RegQueryStringValue(RootKey, Key, 'PluginPath', Path) or
         RegQueryStringValue(RootKey, Key + '\PluginPath', '', Path) then ScanPlugins(Path);
      if RegQueryStringValue(RootKey, Key, 'ApplicationPath', Path) or
         RegQueryStringValue(RootKey, Key + '\ApplicationPath', '', Path) then
        ScanPlugins(AddBackslash(Path) + 'Plug-ins');
    end;
end;

procedure FindLegacy;
begin
  SetArrayLength(LegacyPaths, 0);
#ifdef TestRoot
  ScanAdobeFolder('{#TestRoot}\Adobe');
  ScanPlugins('{#TestRoot}\Shared');
#else
  ScanAdobeFolder(ExpandConstant('{autopf64}\Adobe'));
  ScanRegistry(HKLM64);
  ScanPlugins(ExpandConstant('{commoncf64}\Adobe\Plug-Ins\CC'));
#endif
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo,
  MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var I: Integer;
begin
  FindLegacy;
  Result := 'Install FITS and XISF for Photoshop:' + NewLine + Space + PluginDirectory('') + NewLine + NewLine;
  if GetArrayLength(LegacyPaths) > 0 then begin
    Result := Result + 'Back up and disable these older copies:' + NewLine;
    for I := 0 to GetArrayLength(LegacyPaths) - 1 do Result := Result + Space + LegacyPaths[I] + NewLine;
    Result := Result + NewLine + 'Backups are kept in ' + BackupBase + NewLine + NewLine;
  end;
  Result := Result + 'Photoshop must be closed. Your preferences are kept.';
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not PhotoshopClosed then
    Result := 'Close Photoshop before installing or updating FITS and XISF. If Photoshop is already closed, restart Windows and try again.';
end;

procedure BackupLegacy;
var I, Suffix: Integer; Target: String; Manifest: TArrayOfString;
begin
  FindLegacy;
  if GetArrayLength(LegacyPaths) = 0 then Exit;
  BackupFolder := BackupBase + '\' + GetDateTimeString('yyyymmdd-hhnnss', '-', ':');
  Suffix := 0;
  while DirExists(BackupFolder) do begin
    Suffix := Suffix + 1;
    BackupFolder := BackupBase + '\' + GetDateTimeString('yyyymmdd-hhnnss', '-', ':') + '-' + IntToStr(Suffix);
  end;
  if not ForceDirectories(BackupFolder) then RaiseException('Cannot create plugin backup folder.');
  SetArrayLength(BackupPaths, GetArrayLength(LegacyPaths));
  SetArrayLength(Manifest, GetArrayLength(LegacyPaths));
  for I := 0 to GetArrayLength(LegacyPaths) - 1 do begin
    Target := BackupFolder + '\' + IntToStr(I + 1) + '-' + ExtractFileName(LegacyPaths[I]);
    if not CopyFile(LegacyPaths[I], Target, True) then RaiseException('Cannot back up ' + LegacyPaths[I]);
    BackupPaths[I] := Target;
    Manifest[I] := Target + ' -> ' + LegacyPaths[I];
    if not SaveStringsToUTF8File(BackupFolder + '\original-paths.txt', Manifest, False) then
      RaiseException('Cannot record plugin backup paths.');
    if not DeleteFile(LegacyPaths[I]) then RaiseException('Cannot replace ' + LegacyPaths[I] + '. Close Photoshop and try again.');
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then begin
    if not PhotoshopClosed then RaiseException('Close Photoshop and try again.');
    BackupLegacy;
  end;
  if CurStep = ssDone then InstallSucceeded := True;
end;

procedure DeinitializeSetup;
var I: Integer;
begin
  if not InstallSucceeded then
    for I := 0 to GetArrayLength(BackupPaths) - 1 do
      if (BackupPaths[I] <> '') and not FileExists(LegacyPaths[I]) then
        if not CopyFile(BackupPaths[I], LegacyPaths[I], True) then
          Log('Could not restore ' + LegacyPaths[I] + '; backup remains at ' + BackupPaths[I]);
end;

function InitializeUninstall: Boolean;
begin
  Result := PhotoshopClosed;
  if not Result then SuppressibleMsgBox('Close Photoshop before uninstalling FITS and XISF.', mbError, MB_OK, IDOK);
end;
