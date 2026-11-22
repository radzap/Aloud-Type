# Listening fault rules
Type: grilling
Status: resolved
Blocked by: 04

## Question
Which conditions trigger a Listening fault, with what timeouts (such as silence duration), what reason text is shown for each known cause, and when does the indicator recover or dismiss?

## Answer

Decided by grilling with the owner (2026-11-22), building on [Audio capture and failure detection](04-audio-capture-and-faults.md).

1. **Two kinds of Listening fault.** *Ending* faults (the mic can't be heard at all) stop capture and end the Dictation. The *recoverable* fault "No sound detected" keeps listening and clears on its own as soon as a voice is heard. Both turn the indicator red and grow it.
2. **Timing.** "No sound detected" fires only if no voice is heard in the first **4 s** of a Dictation. Silence after a voice has been heard once never faults. Broken-mic checks apply at any point: no packets for **> 1 s** (stall) and exact digital silence for **4 s** are both ending faults.
3. **Selected mic missing, others present.** Fall back to System default for this Dictation and show a non-red note "Using default microphone" for about 2 s. The saved choice is kept. No mic at all is the "No microphone found" fault.
4. **Dismissal.** Hold mode: red while the key is held, then lingers **2 s** after release. Tap-to-latch mode: an ending fault auto-dismisses after **5 s**, or on the next tap, which only dismisses and doesn't start a Dictation. The recoverable fault clears when a voice is heard or the Dictation ends normally.
5. **Reason texts** (English, one line):

   | Message | Research reasons |
   |---|---|
   | No microphone found | 1 |
   | Microphone disconnected | 3 |
   | Microphone in use by another app | 4 |
   | Windows is blocking the microphone | 5, 6 |
   | Microphone is muted or silent | 9, 10 |
   | No sound detected (recoverable) | 11 |
   | Microphone error | 7, 8, 12 |

   Raw HRESULT/ErrorKind goes to the local log only.
6. **Help.** The overlay stays click-through with text only. While "Windows is blocking the microphone" stands, the tray menu gains "Open microphone privacy settings". Nothing opens automatically.
7. **Text around an ending fault.** Already-typed text stays. Audio captured before the fault is flushed through the recognizer and typed.
8. **Privacy pre-check** at Dictation start reads the consent/policy registry values. Only an explicit Deny faults (without opening the stream); missing or unknown values fall through. Confirmed by the on-device test spike.
9. **Tray icon.** After an ending fault, the tray icon shows its fault state, with the reason in the tooltip, until the next Dictation starts successfully. The recoverable fault never changes the tray icon.
10. **No throttling.** Every key press retries and shows the fault again if it persists.
11. **Key press during a lingering fault** closes the indicator and starts a new Dictation with a fresh pre-check, except for the latched dismiss tap in point 4.
