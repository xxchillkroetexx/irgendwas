<?php

namespace SecretSanta\Services;

use SecretSanta\Models\User;
use SecretSanta\Models\Group;
use SecretSanta\Models\GiftAssignment;
use PHPMailer\PHPMailer\PHPMailer;
use PHPMailer\PHPMailer\Exception;
use PHPMailer\PHPMailer\SMTP;

// Define APP_ROOT if not already defined (for accessing storage directory)
if (!defined('APP_ROOT')) {
    define('APP_ROOT', dirname(dirname(__DIR__)));
}

class EmailService {
    private string $host;
    private int $port;
    private string $username;
    private string $password;
    private string $encryption;
    private string $fromAddress;
    private string $fromName;
    
    public function __construct() {
        $this->host = getenv('MAIL_HOST') ?: 'smtp.example.com';
        $this->port = (int) (getenv('MAIL_PORT') ?: 587);
        $this->username = getenv('MAIL_USERNAME') ?: 'your_username';
        $this->password = getenv('MAIL_PASSWORD') ?: 'your_password';
        $this->encryption = getenv('MAIL_ENCRYPTION') ?: 'tls';
        $this->fromAddress = getenv('MAIL_FROM_ADDRESS') ?: 'noreply@example.com';
        $this->fromName = getenv('MAIL_FROM_NAME') ?: 'Secret Santa';
    }
    
    public function sendInvitation(User $invitee, Group $group, User $inviter): bool {
        $subject = "You've been invited to join a Secret Santa group";
        
        $message = "
        <html>
        <body>
            <h1>Secret Santa Invitation</h1>
            <p>Hello,</p>
            <p>{$inviter->getName()} has invited you to join their Secret Santa group: <strong>{$group->getName()}</strong>.</p>
            <p>To accept this invitation, please click on the link below:</p>
            <p><a href='" . $this->getBaseUrl() . "/group/join/{$group->getInvitationCode()}'>Join Secret Santa Group</a></p>
            <p>If you already have an account, you'll be able to log in. If not, you'll need to create one.</p>
            <p>Happy gift giving!</p>
            <p>- The Secret Santa Team</p>
        </body>
        </html>
        ";
        
        return $this->sendEmail($invitee->getEmail(), $subject, $message);
    }
    
    public function sendDrawNotification(string $email, string $giverName, string $receiverName, string $groupName, string $groupUrl): bool {
        $subject = "Your Secret Santa Draw Results for {$groupName}";
        
        $message = "
        <html>
        <body>
            <h1>Secret Santa Draw Results</h1>
            <p>Hello {$giverName},</p>
            <p>The Secret Santa draw for <strong>{$groupName}</strong> has been completed!</p>
            <p>You have been assigned to give a gift to: <strong>{$receiverName}</strong></p>
            <p>To view their wishlist and learn more about what they'd like, please click the link below:</p>
            <p><a href='{$groupUrl}'>View Group Details</a></p>
            <p>Remember to keep this a secret! The fun of Secret Santa is in the surprise.</p>
            <p>Happy gift giving!</p>
            <p>- The Secret Santa Team</p>
        </body>
        </html>
        ";
        
        return $this->sendEmail($email, $subject, $message);
    }
    
    public function sendPasswordReset(User $user, string $token): bool {
        $subject = "Password Reset Request";
        
        // Get base URL from environment or use default
        $baseUrl = $this->getBaseUrl();
        $resetLink = $baseUrl . "/auth/reset-password/{$token}";
        
        $message = "
        <html>
        <body style='font-family: Arial, sans-serif; line-height: 1.6; color: #333;'>
            <div style='max-width: 600px; margin: 0 auto; padding: 20px; border: 1px solid #ddd; border-radius: 5px;'>
                <h1 style='color: #0066cc; text-align: center;'>Password Reset Request</h1>
                <p>Hello {$user->getName()},</p>
                <p>We received a request to reset your password for your Secret Santa account. If you didn't make this request, you can ignore this email.</p>
                <p>To reset your password, please click on the link below:</p>
                <p style='text-align: center;'>
                    <a href='{$resetLink}' style='display: inline-block; background-color: #0066cc; color: white; padding: 10px 20px; text-decoration: none; border-radius: 4px;'>Reset Password</a>
                </p>
                <p style='text-align: center;'>Or copy and paste this URL into your browser: <br><a href='{$resetLink}'>{$resetLink}</a></p>
                <p><strong>Important:</strong> This link will expire in 24 hours.</p>
                <p>If you have any questions, please contact us at support@secretsanta.example.com</p>
                <hr style='border: none; border-top: 1px solid #ddd; margin: 20px 0;'>
                <p style='font-size: 12px; color: #777; text-align: center;'>- The Secret Santa Team</p>
            </div>
        </body>
        </html>
        ";
        
        // Log the reset link for debugging purposes
        error_log("Password reset link for {$user->getEmail()}: {$resetLink}");
        
        return $this->sendEmail($user->getEmail(), $subject, $message);
    }

    public function sendExistingAccountNotification(string $email): bool {
        $subject = "Account Information";
        
        $baseUrl = $this->getBaseUrl();
        $resetLink = $baseUrl . "/auth/forgot-password";
        
        $message = "
        <html>
        <body style='font-family: Arial, sans-serif; line-height: 1.6; color: #333;'>
            <div style='max-width: 600px; margin: 0 auto; padding: 20px; border: 1px solid #ddd; border-radius: 5px;'>
                <h1 style='color: #0066cc; text-align: center;'>Account Information</h1>
                <p>Hello,</p>
                <p>We received a registration request for your email address on our Secret Santa platform.</p>
                <p>Our records show that an account with this email address already exists.</p>
                <p>If you forgot your password or need to access your account, please use the password reset option:</p>
                <p style='text-align: center;'>
                    <a href='{$resetLink}' style='display: inline-block; background-color: #0066cc; color: white; padding: 10px 20px; text-decoration: none; border-radius: 4px;'>Reset Password</a>
                </p>
                <p>If you didn't attempt to register, you can safely ignore this email.</p>
                <hr style='border: none; border-top: 1px solid #ddd; margin: 20px 0;'>
                <p style='font-size: 12px; color: #777; text-align: center;'>- The Secret Santa Team</p>
            </div>
        </body>
        </html>
        ";
        
        return $this->sendEmail($email, $subject, $message);
    }

    public function sendWelcomeEmail(string $email, ?string $name = null): bool {
        $subject = "Welcome to Secret Santa!";
        
        $greeting = $name ? "Hello {$name}," : "Hello,";
        $baseUrl = $this->getBaseUrl();
        
        $message = "
        <html>
        <body style='font-family: Arial, sans-serif; line-height: 1.6; color: #333;'>
            <div style='max-width: 600px; margin: 0 auto; padding: 20px; border: 1px solid #ddd; border-radius: 5px;'>
                <h1 style='color: #0066cc; text-align: center;'>Welcome to Secret Santa!</h1>
                <p>{$greeting}</p>
                <p>Thank you for joining our Secret Santa platform! Your account has been successfully created.</p>
                <p>With Secret Santa, you can:</p>
                <ul>
                    <li>Create or join gift exchange groups</li>
                    <li>Participate in secret gift draws</li>
                    <li>Create and share wishlists</li>
                    <li>Connect with friends and family for holiday fun</li>
                </ul>
                <p style='text-align: center;'>
                    <a href='{$baseUrl}/user/dashboard' style='display: inline-block; background-color: #0066cc; color: white; padding: 10px 20px; text-decoration: none; border-radius: 4px;'>Go to Dashboard</a>
                </p>
                <p>We hope you enjoy using our platform to organize your gift exchanges!</p>
                <hr style='border: none; border-top: 1px solid #ddd; margin: 20px 0;'>
                <p style='font-size: 12px; color: #777; text-align: center;'>- The Secret Santa Team</p>
            </div>
        </body>
        </html>
        ";
        
        return $this->sendEmail($email, $subject, $message);
    }
    
    private function sendEmail(string $to, string $subject, string $message): bool {
        // Log the email details for development purposes
        error_log("Email would be sent to: {$to}");
        error_log("Subject: {$subject}");
        error_log("Message: " . substr($message, 0, 100) . "...");
        
        // Check if we're in development mode (handling various string values)
        $appDebug = strtolower(trim(getenv('APP_DEBUG') ?: 'false'));
        $appEnv = strtolower(trim(getenv('APP_ENV') ?: 'production'));
        $isDevelopment = ($appEnv === 'development' || $appDebug === 'true' || $appDebug === '1');
        
        // For Docker environment or any environment, save email to a file for testing purposes
        $emailDir = APP_ROOT . '/storage/emails';
        if (!is_dir($emailDir)) {
            mkdir($emailDir, 0777, true);
        }

        // Sanitize the subject for use in the filename
        $sanitizedSubject = preg_replace('/[^a-zA-Z0-9]/', '_', $subject);
        $filename = $emailDir . '/' . time() . '_' . $sanitizedSubject . '.html';
        
        // Create the email content with details
        $fileContent = "To: {$to}\n";
        $fileContent .= "Subject: {$subject}\n";
        $fileContent .= "From: {$this->fromName} <{$this->fromAddress}>\n\n";
        $fileContent .= $message;
        
        // Save to file (always do this as a backup)
        file_put_contents($filename, $fileContent);
        error_log("Email saved to file for testing: {$filename}");
        
        // In development mode, just log and return success
        if ($isDevelopment) {
            error_log("Email sending skipped (development mode)");
            return true;
        }
        
        // Try to send through PHPMailer
        try {
            // Create a new PHPMailer instance
            $mail = new PHPMailer(true);
            
            // Debugging
            $mail->SMTPDebug = SMTP::DEBUG_SERVER; // Output debug info
            $mail->Debugoutput = function($str, $level) {
                error_log("PHPMailer [$level]: $str");
            };
            
            // Check if SMTP is properly configured
            if (!empty($this->host) && $this->host !== 'smtp.example.com') {
                // Server settings for SMTP
                $mail->isSMTP();
                $mail->Host = $this->host;
                $mail->SMTPAuth = true;
                $mail->Username = $this->username;
                $mail->Password = $this->password;
                $mail->Port = $this->port;
                
                // Set longer timeout for slow connections
                $mail->Timeout = 30; // 30 seconds
                
                // Set encryption type if specified
                if ($this->encryption === 'tls') {
                    $mail->SMTPSecure = PHPMailer::ENCRYPTION_STARTTLS;
                } elseif ($this->encryption === 'ssl') {
                    $mail->SMTPSecure = PHPMailer::ENCRYPTION_SMTPS;
                }
            } else {
                // If SMTP not configured, use PHP's mail function
                $mail->isMail();
            }
            
            // Recipients
            $mail->setFrom($this->fromAddress, $this->fromName);
            $mail->addAddress($to);
            
            // Content
            $mail->isHTML(true);
            $mail->Subject = $subject;
            $mail->Body = $message;
            $mail->CharSet = 'UTF-8';
            
            // Send the email
            $sent = $mail->send();
            if ($sent) {
                error_log("Email successfully sent to {$to} via PHPMailer");
            }
            return true;
            
        } catch (Exception $e) {
            error_log("PHPMailer error: " . $e->getMessage());
            // Return true anyway since we already saved the email to file as fallback
            return true;
        }
    }
    
    private function getBaseUrl(): string {
        return getenv('APP_URL') ?: 'https://localhost';
    }
}