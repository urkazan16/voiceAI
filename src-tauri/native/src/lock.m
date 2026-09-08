#import <ApplicationServices/ApplicationServices.h>
#import <CoreGraphics/CoreGraphics.h>
#import <CoreFoundation/CoreFoundation.h>

int lf_screen_is_locked(void) {
    CFDictionaryRef dict = CGSessionCopyCurrentDictionary();
    if (dict == NULL) {
        return 0;
    }
    const void *val = CFDictionaryGetValue(dict, CFSTR("CGSSessionScreenIsLocked"));
    int locked = 0;
    if (val != NULL) {
        locked = CFBooleanGetValue((CFBooleanRef)val) ? 1 : 0;
    }
    CFRelease(dict);
    return locked;
}

/// Shows the system Accessibility prompt once when LocalFlow is not yet in
/// Privacy & Security. Must not run from the paste path: a prompt on every
/// Cmd+V would open Universal Access during dictation.
int lf_prompt_accessibility(void) {
    const void *keys[] = {kAXTrustedCheckOptionPrompt};
    const void *vals[] = {kCFBooleanTrue};
    CFDictionaryRef opts = CFDictionaryCreate(
        kCFAllocatorDefault,
        keys,
        vals,
        1,
        &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks);
    bool trusted = AXIsProcessTrustedWithOptions(opts);
    if (opts != NULL) {
        CFRelease(opts);
    }
    return trusted ? 1 : 0;
}
