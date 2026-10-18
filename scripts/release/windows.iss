#define AppVersion GetEnv("ECHORA_VERSION")
#define NumericVersion GetEnv("ECHORA_NUMERIC_VERSION")
#define Payload GetEnv("ECHORA_PAYLOAD")
#define Output GetEnv("ECHORA_OUTPUT")

[Setup]
AppId={{0796A8DC-5A61-4917-8F13-8E64A208A57C}
AppName=Echora
AppVersion={#AppVersion}
AppVerName=Echora {#AppVersion}
AppPublisher=Echora contributors
AppPublisherURL=https://github.com/rita152/Echora
DefaultDirName={localappdata}\Programs\Echora
DefaultGroupName=Echora
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\gpui-chat-clone.exe
VersionInfoVersion={#NumericVersion}
OutputDir={#Output}
OutputBaseFilename=Echora-{#AppVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
SetupLogging=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Payload}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Echora"; Filename: "{app}\gpui-chat-clone.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Echora"; Filename: "{app}\gpui-chat-clone.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\gpui-chat-clone.exe"; Description: "Launch Echora"; Flags: nowait postinstall skipifsilent
