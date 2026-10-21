; inputlog_ahk.ahk -- AutoHotkey v2 recorder for the WinLinez black-box phase.
;
; Usage:
;     AutoHotkey inputlog_ahk.ahk C:\bb\captures\session1.csv
;
; Controls:
;     Ctrl+Alt+R   start / stop recording
;     F8           drop a MARKER row (use it to segment the capture)
;
; Output columns: t_ms, kind, key, vk, x, y, extra
;   kind: KEYDOWN | KEYUP | MOUSEDOWN | MOUSEUP | MOUSEMOVE | MARKER

#Requires AutoHotkey v2.1
#SingleInstance Force

outFile := A_Args.Length ? A_Args[1] : "inputlog.csv"

; ---------------------------------------------------------------- state
recording := false
startTime := 0
keyState := Map()
lButtonDown := false

; ---------------------------------------------------------------- helpers
now_ms() => (GetTickCount64() - startTime)

openLog() {
    global outFile
    SplitPath(outFile, , , &ext)
    if ext = "" {
        SplitPath(outFile, &dir, &name, &ext)
        outFile := dir (ext = "" ? "" : "." ext) name (ext = "" ? ".csv" : "")
        ; fall back to a plain path when no extension was given
    }
    SplitPath(outFile, &dir)
    if dir != "" DirCreate(dir)
    return FileOpen(outFile, "w", "UTF-8")
}

log := openLog()
log.Write("t_ms,kind,key,vk,x,y,extra")

vkName(vk) {
    static m := Map(
        "LButton", "LBUTTON", "RButton", "RBUTTON", "MButton", "MBUTTON",
        "Escape", "ESC", "Enter", "ENTER", "Tab", "TAB", "Space", "SPACE",
        "LShift", "LSHIFT", "LCtrl", "LCtrl", "LAlt", "LALT", "RAlt", "RALT")
    if m.Has(vk) return m[vk]
    if vk >= "F1" && vk <= "F24" {
        n := RegExMatch(vk, "\d+")
        return vk
    }
    return vk
}

; ---------------------------------------------------------------- hotkeys
^!r:: {
    global recording, startTime
    if recording {
        recording := false
        log.Write(now_ms() ",STOP,,,,")
        log.Flush()
    } else {
        startTime := GetTickCount64()
        recording := true
        log.Write(now_ms() ",START,,,,")
        log.Flush()
    }
}

F8:: {
    global recording
    if recording {
        log.Write(now_ms() ",MARKER,F8,,,,")
        log.Flush()
    }
}

; ---------------------------------------------------------------- keyboard
~*a:: {
    global recording, keyState
    if !recording return
    vk := Format("0x{:X}", A_KeySC & 0xFF)
    log.Write(now_ms() ",KEYDOWN," keyName(A_KeyName) "," vk ",,,")
    log.Flush()
}

; ---------------------------------------------------------------- mouse
~*LButton:: {
    global recording
    if recording {
        MouseGetPos(&x, &y)
        log.Write(now_ms() ",MOUSEDOWN,LBUTTON,1," x "," y ","")
        log.Flush()
    }
}

~*LButtonUp:: {
    global recording
    if recording {
        MouseGetPos(&x, &y)
        log.Write(now_ms() ",MOUSEUP,LBUTTON,1," x "," y ","")
        log.Flush()
    }
}

; poll the mouse position so hover behaviour is captured too
SetTimer(trackMouse, 16)

trackMouse() {
    global recording, lButtonDown
    if !recording return
    MouseGetPos(&x, &y)
    down := GetKeyState("LButton", "P")
    if down && !lButtonDown {
        log.Write(now_ms() ",MOUSEDOWN,LBUTTON,1," x "," y ","")
    } else if !down && lButtonDown {
        log.Write(now_ms() ",MOUSEUP,LBUTTON,1," x "," y ","")
    }
    lButtonDown := down
    log.Write(now_ms() ",MOUSEMOVE,,," x "," y ","")
}
