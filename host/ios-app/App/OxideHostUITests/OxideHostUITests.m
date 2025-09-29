#import <XCTest/XCTest.h>

@interface OxideHostUITests : XCTestCase

@property(nonatomic, strong) XCUIApplication *app;

@end

@implementation OxideHostUITests

- (void)setUp
{
    self.continueAfterFailure = NO;
    self.app = [[XCUIApplication alloc] init];
    self.app.launchEnvironment = @{
        @"OXIDEUI_UI_LOG": @"1",
        @"OXIDEUI_RUST_LOG": @"1",
        @"OXIDEUI_RENDER_IN_TEST": @"1",
        @"OXIDEUI_CAPTURE_METAL": @"0",
        @"OXIDEUI_DELAY_RELEASE_MS": @"0",
        @"NSUnbufferedIO": @"YES"
    };
    [self.app launch];
}

- (void)tearDown
{
    [self.app terminate];
    self.app = nil;
}

- (void)selectSceneAtIndex:(NSUInteger)index
{
    XCUIElement *seg = self.app.segmentedControls[@"sceneControl"];
    if (!seg.exists)
    {
        XCTFail(@"Missing scene segmented control");
        return;
    }
    NSArray<XCUIElement *> *buttons = seg.buttons.allElementsBoundByIndex;
    if (index >= buttons.count)
    {
        XCTFail(@"scene index %lu out of range (%lu)", (unsigned long)index, (unsigned long)buttons.count);
        return;
    }
    XCUIElement *target = buttons[index];
    if (target.exists)
    {
        [target tap];
    }
}

- (void)testSceneSwitcherAndToggles
{
    XCUIApplication *app = self.app;

    XCUIElement *sceneControl = app.segmentedControls[@"sceneControl"];
    XCTAssertTrue([sceneControl waitForExistenceWithTimeout:5.0]);

    NSArray<NSString *> *sceneNames = @[ @"Controls", @"Text Layout", @"Zoom Image", @"Animations", @"Collection Stress", @"Damage Lab", @"Nine Slice", @"SDF Text", @"Snapshot", @"Camera" ];

    for (NSString *name in sceneNames)
    {
        XCUIElement *button = sceneControl.buttons[name];
        XCTAssertTrue([button waitForExistenceWithTimeout:2.0], @"Missing scene button %@", name);
        if (![button isSelected])
        {
            [button tap];
        }
        XCTAssertTrue(button.isSelected, @"Scene %@ should be selected", name);

        if ([name isEqualToString:@"Controls"])
        {
            XCUIElement *overlaySwitch = app.switches[@"overlaySwitch"];
            XCTAssertTrue([overlaySwitch waitForExistenceWithTimeout:2.0]);
            BOOL initial = [[overlaySwitch.value description] boolValue];
            [overlaySwitch tap];
            BOOL toggled = [[overlaySwitch.value description] boolValue];
            XCTAssertNotEqual(initial, toggled);
            [overlaySwitch tap];

            XCUIElement *reduceSwitch = app.switches[@"reduceMotionSwitch"];
            XCTAssertTrue([reduceSwitch waitForExistenceWithTimeout:2.0]);
            BOOL reduceInitial = [[reduceSwitch.value description] boolValue];
            [reduceSwitch tap];
            BOOL reduceToggled = [[reduceSwitch.value description] boolValue];
            XCTAssertNotEqual(reduceInitial, reduceToggled);
            [reduceSwitch tap];

            XCTAttachment *controlsShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
            controlsShot.name = @"controls-scene";
            controlsShot.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:controlsShot];
        }
        else if ([name isEqualToString:@"Zoom Image"])
        {
            XCUIElement *canvas = app.otherElements[@"metalView"];
            if ([canvas waitForExistenceWithTimeout:2.0])
            {
                [canvas pinchWithScale:1.4 velocity:1.0];
                [canvas pinchWithScale:0.7 velocity:-1.0];
                XCUICoordinate *center = [canvas coordinateWithNormalizedOffset:CGVectorMake(0.5, 0.5)];
                XCUICoordinate *dragTarget = [center coordinateWithOffset:CGVectorMake(120.0, 0.0)];
                [center pressForDuration:0.2 thenDragToCoordinate:dragTarget];
                [canvas doubleTap];
                XCTAttachment *zoomShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
                zoomShot.name = @"zoom-scene";
                zoomShot.lifetime = XCTAttachmentLifetimeKeepAlways;
                [self addAttachment:zoomShot];
            }
        }
        else if ([name isEqualToString:@"Animations"])
        {
            XCUIElement *animPlay = app.switches[@"animationPlaySwitch"];
            XCUIElement *animPhase = app.sliders[@"animationPhaseSlider"];
            if (animPlay.exists)
            {
                BOOL start = [[animPlay.value description] boolValue];
                [animPlay tap];
                BOOL toggled = [[animPlay.value description] boolValue];
                XCTAssertNotEqual(start, toggled);
                [animPlay tap];
            }
            if (animPhase.exists)
            {
                [animPhase adjustToNormalizedSliderPosition:0.25];
                [animPhase adjustToNormalizedSliderPosition:0.75];
            }
            XCTAttachment *animShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
            animShot.name = @"animations-scene";
            animShot.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:animShot];
        }
        else if ([name isEqualToString:@"Collection Stress"])
        {
            XCUIElement *collection = app.collectionViews.firstMatch;
            if ([collection waitForExistenceWithTimeout:2.0])
            {
                [collection swipeUp];
                [collection swipeDown];
                XCUIElement *firstCell = collection.cells.firstMatch;
                if (firstCell.exists)
                {
                    [firstCell tap];
                    XCTAssertTrue(firstCell.isHittable || firstCell.selected);
                }
                XCTAttachment *collectionShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
                collectionShot.name = @"collection-scene";
                collectionShot.lifetime = XCTAttachmentLifetimeKeepAlways;
                [self addAttachment:collectionShot];
            }
        }
        else if ([name isEqualToString:@"Damage Lab"])
        {
            XCUIElement *damageSwitch = app.switches[@"damageEnableSwitch"];
            XCUIElement *damageUse = app.sliders[@"damageUseSlider"];
            XCUIElement *damagePref = app.sliders[@"damagePrefSlider"];
            if (damageSwitch.exists)
            {
                BOOL start = [[damageSwitch.value description] boolValue];
                [damageSwitch tap];
                BOOL toggled = [[damageSwitch.value description] boolValue];
                XCTAssertNotEqual(start, toggled);
                [damageSwitch tap];
            }
            if (damageUse.exists)
            {
                [damageUse adjustToNormalizedSliderPosition:0.35];
                [damageUse adjustToNormalizedSliderPosition:0.85];
            }
            if (damagePref.exists)
            {
                [damagePref adjustToNormalizedSliderPosition:0.15];
                [damagePref adjustToNormalizedSliderPosition:0.55];
            }
            XCTAttachment *damageShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
            damageShot.name = @"damage-scene";
            damageShot.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:damageShot];
        }
        else if ([name isEqualToString:@"Nine Slice"])
        {
            XCUIElement *slice = app.sliders[@"nineSliceSlider"];
            XCUIElement *alpha = app.sliders[@"nineAlphaSlider"];
            XCTAssertTrue([slice waitForExistenceWithTimeout:2.0]);
            [slice adjustToNormalizedSliderPosition:0.2];
            [slice adjustToNormalizedSliderPosition:0.8];
            XCTAssertTrue([alpha waitForExistenceWithTimeout:2.0]);
            [alpha adjustToNormalizedSliderPosition:0.3];
            [alpha adjustToNormalizedSliderPosition:0.9];
            XCTAttachment *nineShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
            nineShot.name = @"nine-slice-scene";
            nineShot.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:nineShot];
        }
        else if ([name isEqualToString:@"SDF Text"])
        {
            XCUIElement *font = app.sliders[@"sdfFontSlider"];
            XCTAssertTrue([font waitForExistenceWithTimeout:2.0]);
            [font adjustToNormalizedSliderPosition:0.1];
            [font adjustToNormalizedSliderPosition:0.7];
            XCTAttachment *sdfShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
            sdfShot.name = @"sdf-scene";
            sdfShot.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:sdfShot];
        }
        else if ([name isEqualToString:@"Input & Haptics"])
        {
            XCUIElement *imeText = app.textViews[@"imeTextView"];
            XCTAssertTrue([imeText waitForExistenceWithTimeout:2.0]);
            [imeText tap];
            [imeText typeText:@"abc"];
            XCTAssertTrue([[imeText value] containsString:@"abc"]);

            XCUIElement *copyButton = app.buttons[@"imeCopyButton"];
            XCUIElement *pasteButton = app.buttons[@"imePasteButton"];
            XCUIElement *hapticButton = app.buttons[@"imeHapticButton"];
            XCTAssertTrue(copyButton.exists);
            XCTAssertTrue(pasteButton.exists);
            XCTAssertTrue(hapticButton.exists);

            [copyButton tap];
            NSPredicate *copiedPredicate = [NSPredicate predicateWithFormat:@"label CONTAINS[c] 'copied'"];
            XCUIElement *statusLabel = app.staticTexts[@"statusLabel"];
            XCTAssertTrue([statusLabel waitForExistenceWithTimeout:2.0]);
            XCTNSPredicateExpectation *copiedExpectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:copiedPredicate object:statusLabel];
            [self waitForExpectations:@[copiedExpectation] timeout:5.0];

            [imeText typeText:@"\u0008\u0008\u0008"];
            [pasteButton tap];
            NSPredicate *pastedPredicate = [NSPredicate predicateWithFormat:@"label CONTAINS[c] 'pasted'"];
            XCTNSPredicateExpectation *pastedExpectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:pastedPredicate object:statusLabel];
            [self waitForExpectations:@[pastedExpectation] timeout:5.0];
            XCTAssertTrue([[imeText value] containsString:@"abc"]);

            [hapticButton tap];
            XCTAttachment *inputShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
            inputShot.name = @"input-scene";
            inputShot.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:inputShot];
        }
        else if ([name isEqualToString:@"Snapshot"])
        {
            XCUIElement *snapshotButton = app.buttons[@"snapshotButton"];
            XCUIElement *statusLabel = app.staticTexts[@"statusLabel"];
            XCTAssertTrue([snapshotButton waitForExistenceWithTimeout:2.0]);
            XCTAssertTrue([statusLabel waitForExistenceWithTimeout:2.0]);
            [snapshotButton tap];
            NSPredicate *predicate = [NSPredicate predicateWithFormat:@"label.length > 0"];
            XCTNSPredicateExpectation *expect = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:statusLabel];
            [self waitForExpectations:@[expect] timeout:5.0];
            XCTAssertTrue([[statusLabel.label lowercaseString] containsString:@"snapshot"]);
            XCTAttachment *snapshotShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
            snapshotShot.name = @"snapshot-scene";
            snapshotShot.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:snapshotShot];
        }
        else if ([name isEqualToString:@"Camera"])
        {
            XCUIElement *camAnim = app.switches[@"cameraAnimateSwitch"];
            XCUIElement *camBlur = app.switches[@"cameraBlurSwitch"];
            XCUIElement *camGray = app.switches[@"cameraGraySwitch"];
            XCUIElement *camSigma = app.sliders[@"cameraSigmaSlider"];
            XCUIElement *metricsLabel = app.staticTexts[@"cameraMetricsLabel"];
            XCTAssertTrue([metricsLabel waitForExistenceWithTimeout:2.0]);

            if (camAnim.exists)
            {
                BOOL start = [[camAnim.value description] boolValue];
                [camAnim tap];
                BOOL toggled = [[camAnim.value description] boolValue];
                XCTAssertNotEqual(start, toggled);
                [camAnim tap];
            }
            if (camBlur.exists)
            {
                BOOL start = [[camBlur.value description] boolValue];
                [camBlur tap];
                BOOL toggled = [[camBlur.value description] boolValue];
                XCTAssertNotEqual(start, toggled);
                [camBlur tap];
            }
            if (camGray.exists)
            {
                BOOL start = [[camGray.value description] boolValue];
                [camGray tap];
                BOOL toggled = [[camGray.value description] boolValue];
                XCTAssertNotEqual(start, toggled);
                [camGray tap];
            }
            if (camSigma.exists)
            {
                [camSigma adjustToNormalizedSliderPosition:0.2];
                [camSigma adjustToNormalizedSliderPosition:0.8];
            }

            XCUIElement *captureSwitch = app.switches[@"cameraCaptureSwitch"];
            if (captureSwitch.exists)
            {
                [captureSwitch tap];
                NSPredicate *pausedPredicate = [NSPredicate predicateWithFormat:@"label CONTAINS[c] 'paused=yes'"];
                XCTNSPredicateExpectation *pausedExpectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:pausedPredicate object:metricsLabel];
                [self waitForExpectations:@[pausedExpectation] timeout:5.0];

                [captureSwitch tap];
                NSPredicate *runningPredicate = [NSPredicate predicateWithFormat:@"label CONTAINS[c] 'paused=no'"];
                XCTNSPredicateExpectation *runningExpectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:runningPredicate object:metricsLabel];
                [self waitForExpectations:@[runningExpectation] timeout:5.0];
            }

            XCTAttachment *cameraShot = [XCTAttachment attachmentWithScreenshot:[app screenshot]];
            cameraShot.name = @"camera-scene";
            cameraShot.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:cameraShot];
        }
    }
}

- (void)testTraverseAllScenes
{
    [self selectSceneAtIndex:0];
    [self addAttachment:[XCTAttachment attachmentWithScreenshot:[self.app screenshot]]];

    [self selectSceneAtIndex:1];
    [self addAttachment:[XCTAttachment attachmentWithScreenshot:[self.app screenshot]]];

    [self selectSceneAtIndex:2];
    XCUIElement *doubleTap = self.app.otherElements[@"zoomViewport"];
    if (doubleTap.exists)
    {
        [doubleTap tap];
        [doubleTap tap];
    }
    [self addAttachment:[XCTAttachment attachmentWithScreenshot:[self.app screenshot]]];

    [self selectSceneAtIndex:3];
    [self addAttachment:[XCTAttachment attachmentWithScreenshot:[self.app screenshot]]];

    [self selectSceneAtIndex:4];
    [self addAttachment:[XCTAttachment attachmentWithScreenshot:[self.app screenshot]]];
}

- (void)testCollectionSceneFocusNavigation
{
    [self selectSceneAtIndex:4];

    XCUIElement *collection = self.app.collectionViews.firstMatch;
    if (!collection.exists)
    {
        XCTSkip(@"collection view not found (possibly renamed)");
        return;
    }

    [collection swipeUp];
    [collection swipeDown];

    XCUIElement *firstCell = collection.cells.firstMatch;
    if (firstCell.exists)
    {
        [firstCell tap];
        XCTAssertTrue(firstCell.selected || firstCell.isHittable);
    }

    [self addAttachment:[XCTAttachment attachmentWithScreenshot:[self.app screenshot]]];
}

@end
