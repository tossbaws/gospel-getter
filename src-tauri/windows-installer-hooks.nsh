; Explorer keeps a replaced icon cached until sign-out, so pinned taskbar buttons and shortcuts would show the old one.
!macro NSIS_HOOK_POSTINSTALL
  ; SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST | SHCNF_FLUSH, NULL, NULL)
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x1000, p 0, p 0)'
!macroend
