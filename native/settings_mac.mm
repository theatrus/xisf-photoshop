#import <AppKit/AppKit.h>
#include "preferences.h"

uint32_t chooseDepthMac(const char* title, const std::string& message, uint32_t initial, bool* remember, uint32_t* debayer) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSAlert* alert = [[NSAlert alloc] init];
        alert.messageText = [NSString stringWithUTF8String:title];
        alert.informativeText = [NSString stringWithUTF8String:message.c_str()];
        [alert addButtonWithTitle:initial == 16 ? @"16-bit integer" : @"32-bit float"];
        [alert addButtonWithTitle:initial == 16 ? @"32-bit float" : @"16-bit integer"];
        [alert addButtonWithTitle:@"Cancel"];
        NSPopUpButton* cfa = nil;
        if (debayer) {
            cfa = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(0, 0, 340, 28) pullsDown:NO];
            [cfa addItemsWithTitles:@[@"Keep raw grayscale", @"Debayer to RGB - Auto (metadata)",
                @"Debayer to RGB - RGGB", @"Debayer to RGB - BGGR", @"Debayer to RGB - GRBG", @"Debayer to RGB - GBRG"]];
            [cfa selectItemAtIndex:*debayer];
            alert.accessoryView = cfa;
        }
        if (remember) {
            *remember = false;
            alert.showsSuppressionButton = YES;
            alert.suppressionButton.title = @"Remember choice";
            alert.suppressionButton.state = NSControlStateValueOff;
        }
        const auto response = [alert runModal];
        if (response != NSAlertFirstButtonReturn && response != NSAlertSecondButtonReturn) return 0;
        if (remember) *remember = alert.suppressionButton.state == NSControlStateValueOn;
        if (debayer) *debayer = static_cast<uint32_t>(cfa.indexOfSelectedItem);
        return response == NSAlertFirstButtonReturn ? initial : initial == 16 ? 32 : 16;
    }
}

bool editDefaults(SeizaDefaults& defaults) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSAlert* alert = [[NSAlert alloc] init];
        alert.messageText = @"FITS / XISF - Default settings";
        alert.informativeText = @"Seiza Astronomy Formats 0.4.0 - shared by FITS and XISF.\n\n"
            @"16-bit import rescales the full image range without clipping. Photoshop retains about 15 bits plus an endpoint; precision and original absolute scale are lost.\n\n"
            @"UInt16 export rounds 0..1 to 0..65535 and clips negative/HDR values. Float32 preserves current document values but cannot recover lost import precision.\n\n"
            @"Match document depth follows the current Photoshop 16/32-bit mode.";
        [alert addButtonWithTitle:@"Save defaults"];
        [alert addButtonWithTitle:@"Cancel"];
        NSView* view = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 440, 175)];
        NSButton* cfa = [NSButton checkboxWithTitle:@"Debayer tagged images to RGB (bilinear)" target:nil action:nil];
        cfa.frame = NSMakeRect(0, 148, 440, 22);
        cfa.state = defaults.debayer ? NSControlStateValueOn : NSControlStateValueOff;
        [view addSubview:cfa];
        NSTextField* importLabel = [NSTextField labelWithString:@"Default import depth"];
        importLabel.frame = NSMakeRect(0, 109, 195, 22);
        [view addSubview:importLabel];
        NSTextField* exportLabel = [NSTextField labelWithString:@"Default saved sample type"];
        exportLabel.frame = NSMakeRect(0, 47, 195, 22);
        [view addSubview:exportLabel];
        NSPopUpButton* read = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(200, 107, 240, 26) pullsDown:NO];
        NSPopUpButton* write = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(200, 45, 240, 26) pullsDown:NO];
        for (NSPopUpButton* menu in @[read, write]) {
            if (menu == write) [menu addItemWithTitle:@"Match document depth"];
            [menu addItemsWithTitles:@[@"32-bit floating point", @"16-bit integer"]];
            [view addSubview:menu];
        }
        [read selectItemAtIndex:defaults.readDepth == 16 ? 1 : 0];
        [write selectItemAtIndex:defaults.writeDepth == 16 ? 2 : defaults.writeDepth == 32 ? 1 : 0];
        NSButton* askOpen = [NSButton checkboxWithTitle:@"Ask on every Open" target:nil action:nil];
        askOpen.frame = NSMakeRect(200, 79, 240, 22);
        askOpen.state = defaults.askOnOpen ? NSControlStateValueOn : NSControlStateValueOff;
        [view addSubview:askOpen];
        NSButton* askSave = [NSButton checkboxWithTitle:@"Ask on every Save / Save As" target:nil action:nil];
        askSave.frame = NSMakeRect(200, 17, 240, 22);
        askSave.state = defaults.askOnSave ? NSControlStateValueOn : NSControlStateValueOff;
        [view addSubview:askSave];
        alert.accessoryView = view;
        if ([alert runModal] != NSAlertFirstButtonReturn) return false;
        defaults.readDepth = read.indexOfSelectedItem == 1 ? 16 : 32;
        defaults.writeDepth = write.indexOfSelectedItem == 2 ? 16 : write.indexOfSelectedItem == 1 ? 32 : 0;
        defaults.askOnOpen = askOpen.state == NSControlStateValueOn;
        defaults.askOnSave = askSave.state == NSControlStateValueOn;
        defaults.debayer = cfa.state == NSControlStateValueOn ? 1 : 0;
        return true;
    }
}
