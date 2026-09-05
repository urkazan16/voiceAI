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
