#import <XCTest/XCTest.h>

@interface OxideHostUITests : XCTestCase
@end

@implementation OxideHostUITests

- (void)setUp
{
    self.continueAfterFailure = NO;
}

- (void)testWindowLaunchSmoke
{
    XCUIApplication *app = [[XCUIApplication alloc] init];
    app.launchEnvironment = @{
        @"OXIDE_RENDER_IN_TEST": @"1",
        @"UITEST": @"1"
    };
    [app launch];

    XCUIElement *window = app.windows.firstMatch;
    XCTAssertTrue([window waitForExistenceWithTimeout:5.0]);

    XCTAttachment *screenshot =
        [XCTAttachment attachmentWithScreenshot:[app screenshot]];
    screenshot.name = @"window-launch-smoke";
    screenshot.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:screenshot];
}

@end
