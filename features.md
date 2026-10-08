Playback speed selector. Built: the speed button cycles 0.5×, 0.75×, 1×, 1.25×, 1.5×, 2×, and 3×. The choice is remembered.

Holding down the picture or the stage (the play area) plays at 3× until you let go, then it returns to the speed you selected.

File system explorer in the playlist. Built: Up goes to the parent folder, Back returns to the folder you came from, and a folder row opens that folder. Filters are All, MP3, Video, and Images. The filter is remembered.

The title of the video, audio, or image is drawn in full in the list and on the stage, wrapping onto as many lines as the name needs. The text in the Windows title bar can still be shortened by Windows itself when the window is narrow.

Always on top. Built: the Top button. Remembered.

Transparency. Built: the slider at the right of the filter row, from 45% to 100% opacity. Remembered.

Online video. Built: paste a direct media URL or a YouTube link into the box and press Enter or Open. YouTube links are resolved with yt-dlp, which is installed on this PC. A direct link to an mp4, webm, mp3, and the other formats the player already opens is handed to Windows without yt-dlp.

Transcription — not built. What it would take:

The player would decode the audio (from an mp3, or the audio track inside a video) into 16 kHz mono PCM, then run a speech-to-text model. Whisper is the practical choice for a course: the small model is a few hundred megabytes, the large model is several gigabytes, and a long course is hours of CPU time or much less if a GPU is available. Windows' own speech recognizer is a poor fit for lectures. A cloud API (OpenAI Whisper, Azure Speech) avoids the local model but sends the audio off the machine and costs per minute.

The useful output is not only a text file. It is a timed transcript (SRT or WebVTT) stored next to the media or in the player cache, so the line on screen matches the playhead. Background music, overlapping voices, and strong accents are where it gets words wrong. Nothing in the current player decodes audio into text, so this would be a new pipeline plus a caption surface.

Explorer integration — not built. Can this play inside an Explorer window?

Explorer does not let an app draw its own player into the folder view. The supported version of "play inside Explorer" is a preview handler: a COM DLL registered for the audio and video types, which Explorer loads into the preview pane when you select a file. That DLL runs inside Explorer, so a crash there takes Explorer down with it, and it has to be a DLL rather than this exe. The right-click "Play with Better Media Player" verb is the integration that already exists.

The other direction, hosting an Explorer view inside this player, is a separate Windows control (IExplorerBrowser). That is Explorer inside the player, not the player inside Explorer. The folder list at the bottom of this player is the stand-in for moving between course modules.

Phone listening over Wi-Fi. Built: the Phone button. The player opens a private link on this PC and shows a QR code. The phone has to be on the same Wi-Fi. Open the link in the phone browser, pick a file, and it plays from the PC without copying the course. Seeking works on files the phone can play directly (mp4, webm, mp3, and the other formats the page serves as-is). Other videos are converted with ffmpeg while they stream. Audio only is a switch on the phone page, and the phone remembers where you were in each file. The link includes a token so the rest of the network cannot browse the folder. Windows is asked to allow the port on private networks only. Leave the player open while you listen.
