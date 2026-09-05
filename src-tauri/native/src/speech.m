#import <Foundation/Foundation.h>
#import <Speech/Speech.h>
#include <stdio.h>

#include "localflow_runtime.h"

static int copy_out(NSString *text, char *out, int out_len) {
    if (text == nil || out == NULL || out_len <= 0) {
        return LF_ERR_RUNTIME;
    }
    const char *utf8 = text.UTF8String;
    if (utf8 == NULL) {
        return LF_ERR_RUNTIME;
    }
    snprintf(out, (size_t)out_len, "%s", utf8);
    return LF_OK;
}

static int recognize_url(NSURL *url, BOOL on_device, NSLocale *locale, char *out, int out_len) {
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    __block NSString *final_text = nil;
    __block int rc = LF_ERR_UNSUPPORTED;
    __block SFSpeechRecognizer *held_recognizer = nil;
    __block SFSpeechRecognitionTask *held_task = nil;

    void (^start)(void) = ^{
      SFSpeechRecognizer *recognizer = [[SFSpeechRecognizer alloc] initWithLocale:locale];
      if (recognizer == nil || !recognizer.isAvailable) {
          rc = LF_ERR_UNSUPPORTED;
          dispatch_semaphore_signal(done);
          return;
      }
      held_recognizer = recognizer;

      SFSpeechURLRecognitionRequest *request = [[SFSpeechURLRecognitionRequest alloc] initWithURL:url];
      request.shouldReportPartialResults = NO;
      if ([request respondsToSelector:@selector(setRequiresOnDeviceRecognition:)]) {
          request.requiresOnDeviceRecognition = on_device;
      }

      held_task = [recognizer recognitionTaskWithRequest:request
          resultHandler:^(SFSpeechRecognitionResult *result, NSError *error) {
            if (error != nil) {
                rc = LF_ERR_RUNTIME;
                dispatch_semaphore_signal(done);
                return;
            }
            if (result.isFinal) {
                final_text = result.bestTranscription.formattedString;
                rc = LF_OK;
                dispatch_semaphore_signal(done);
            }
          }];
    };

    if ([NSThread isMainThread]) {
        start();
    } else {
        dispatch_async(dispatch_get_main_queue(), start);
    }

    long timed_out = dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 12 * NSEC_PER_SEC));
    (void)held_task;
    (void)held_recognizer;
    if (timed_out != 0) {
        return LF_ERR_RUNTIME;
    }
    if (rc != LF_OK) {
        return rc;
    }
    if (final_text == nil || final_text.length == 0) {
        return LF_ERR_RUNTIME;
    }
    return copy_out(final_text, out, out_len);
}

int lf_macos_transcribe(const char *wav_path, char *out, int out_len) {
    if (wav_path == NULL || wav_path[0] == '\0' || out == NULL || out_len <= 0) {
        return LF_ERR_RUNTIME;
    }

    @autoreleasepool {
        __block SFSpeechRecognizerAuthorizationStatus auth = SFSpeechRecognizerAuthorizationStatusDenied;
        dispatch_semaphore_t auth_done = dispatch_semaphore_create(0);
        dispatch_async(dispatch_get_main_queue(), ^{
          [SFSpeechRecognizer requestAuthorization:^(SFSpeechRecognizerAuthorizationStatus status) {
            auth = status;
            dispatch_semaphore_signal(auth_done);
          }];
        });
        dispatch_semaphore_wait(auth_done, dispatch_time(DISPATCH_TIME_NOW, 20 * NSEC_PER_SEC));
        if (auth != SFSpeechRecognizerAuthorizationStatusAuthorized) {
            return LF_ERR_PERMISSION;
        }

        NSURL *url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:wav_path]];
        NSMutableArray<NSLocale *> *locales = [NSMutableArray array];
        NSLocale *current = [NSLocale currentLocale];
        NSString *current_id = current.localeIdentifier ?: @"";
        if (current != nil) {
            [locales addObject:current];
        }
        if (![current_id hasPrefix:@"ru"]) {
            [locales addObject:[NSLocale localeWithLocaleIdentifier:@"ru-RU"]];
        }
        if (![current_id hasPrefix:@"en"]) {
            [locales addObject:[NSLocale localeWithLocaleIdentifier:@"en-US"]];
        }

        for (NSLocale *locale in locales) {
            int rc = recognize_url(url, YES, locale, out, out_len);
            if (rc == LF_OK) {
                return LF_OK;
            }
        }
        for (NSLocale *locale in locales) {
            int rc = recognize_url(url, NO, locale, out, out_len);
            if (rc == LF_OK) {
                return LF_OK;
            }
        }
        return LF_ERR_UNSUPPORTED;
    }
}
