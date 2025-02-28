<?php

namespace SecretSanta\Services;

use SecretSanta\Models\User;
use SecretSanta\Models\Group;
use SecretSanta\Models\GiftAssignment;

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
        
        $message = "
        <html>
        <body>
            <h1>Password Reset Request</h1>
            <p>Hello {$user->getName()},</p>
            <p>We received a request to reset your password. If you didn't make this request, you can ignore this email.</p>
            <p>To reset your password, please click on the link below:</p>
            <p><a href='" . $this->getBaseUrl() . "/auth/reset-password/{$token}'>Reset Password</a></p>
            <p>This link will expire in 24 hours.</p>
            <p>- The Secret Santa Team</p>
        </body>
        </html>
        ";
        
        return $this->sendEmail($user->getEmail(), $subject, $message);
    }
    
    private function sendEmail(string $to, string $subject, string $message): bool {
        // In a real application, you would use a library like PHPMailer or SwiftMailer
        // For this example, we'll use a simple implementation with PHP's mail() function
        
        $headers = [
            'MIME-Version: 1.0',
            'Content-type: text/html; charset=UTF-8',
            'From: ' . $this->fromName . ' <' . $this->fromAddress . '>',
            'X-Mailer: PHP/' . phpversion()
        ];
        
        // For demonstration purposes, we'll return true
        // In a real application, you'd implement SMTP sending here
        
        // Debug output to logs
        error_log("Email would be sent to: {$to}");
        error_log("Subject: {$subject}");
        error_log("Message: " . substr($message, 0, 100) . "...");
        
        return true;
    }
    
    private function getBaseUrl(): string {
        return getenv('APP_URL') ?: 'https://localhost';
    }
}