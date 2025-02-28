<?php
namespace models;

use core\Database\Projekt_DB;

class Group {
    private $db;
    
    // Group properties
    private $id;
    private $name;
    private $description;
    private $adminId;
    private $joinDeadline;
    private $drawDate;
    private $isDrawn;
    private $customEmailTemplate;
    private $wishlistVisibility;
    private $createdAt;
    private $updatedAt;
    
    public function __construct($data = null) {
        $this->db = Projekt_DB::getInstance();
        
        if ($data) {
            $this->mapData($data);
        }
    }
    
    // Map data from array or object to properties
    private function mapData($data) {
        $data = (array) $data;
        
        $this->id = $data['id'] ?? null;
        $this->name = $data['name'] ?? null;
        $this->description = $data['description'] ?? null;
        $this->adminId = $data['admin_id'] ?? null;
        $this->joinDeadline = $data['join_deadline'] ?? null;
        $this->drawDate = $data['draw_date'] ?? null;
        $this->isDrawn = (bool) ($data['is_drawn'] ?? false);
        $this->customEmailTemplate = $data['custom_email_template'] ?? null;
        $this->wishlistVisibility = $data['wishlist_visibility'] ?? 'all';
        $this->createdAt = $data['created_at'] ?? null;
        $this->updatedAt = $data['updated_at'] ?? null;
    }
    
    // Find group by ID
    public function findById($id) {
        $group = $this->db->table('groups')
            ->where('id', $id)
            ->first();
            
        if ($group) {
            return new self($group);
        }
        
        return null;
    }
    
    // Create a new group
    public function create($data) {
        // Handle null dates properly
        $joinDeadline = !empty($data['join_deadline']) ? $data['join_deadline'] : null;
        $drawDate = !empty($data['draw_date']) ? $data['draw_date'] : null;
        
        $groupId = $this->db->table('groups')->insert([
            'name' => $data['name'],
            'description' => $data['description'] ?? null,
            'admin_id' => $data['admin_id'],
            'join_deadline' => $joinDeadline,
            'draw_date' => $drawDate,
            'custom_email_template' => $data['custom_email_template'] ?? null,
            'wishlist_visibility' => $data['wishlist_visibility'] ?? 'all'
        ]);
        
        // Only proceed if we have a valid group ID
        if ($groupId) {
            // Add admin as a group member
            $this->db->table('group_members')->insert([
                'group_id' => $groupId,
                'user_id' => $data['admin_id'],
                'status' => 'active'
            ]);
            
            return $this->findById($groupId);
        }
        
        return null;
    }
    
    // Update group
    public function update($data) {
        $this->db->table('groups')
            ->where('id', $this->id)
            ->update($data);
            
        return $this->findById($this->id);
    }
    
    // Delete group
    public function delete() {
        return $this->db->table('groups')
            ->where('id', $this->id)
            ->delete();
    }
    
    // Add member to group
    public function addMember($userId, $email = null) {
        // Check if user is already a member
        $existingMember = $this->db->table('group_members')
            ->where('group_id', $this->id)
            ->where('user_id', $userId)
            ->first();
            
        if ($existingMember) {
            // Update status if previously declined
            if ($existingMember['status'] === 'declined') {
                return $this->db->table('group_members')
                    ->where('id', $existingMember['id'])
                    ->update(['status' => 'active']);
            }
            
            return false; // Already a member
        }
        
        return $this->db->table('group_members')->insert([
            'group_id' => $this->id,
            'user_id' => $userId,
            'invitation_email' => $email,
            'status' => 'active'
        ]);
    }
    
    // Invite member to group
    public function inviteMember($email) {
        // Generate invitation token
        $token = bin2hex(random_bytes(16));
        
        // Check if email is already invited
        $existingInvitation = $this->db->table('group_members')
            ->where('group_id', $this->id)
            ->where('invitation_email', $email)
            ->first();
            
        if ($existingInvitation) {
            // Update token
            return $this->db->table('group_members')
                ->where('id', $existingInvitation['id'])
                ->update([
                    'invitation_token' => $token,
                    'status' => 'invited'
                ]);
        }
        
        // Insert new invitation
        $this->db->table('group_members')->insert([
            'group_id' => $this->id,
            'user_id' => 0, // Will be updated when user accepts
            'invitation_token' => $token,
            'invitation_email' => $email,
            'status' => 'invited'
        ]);
        
        return $token;
    }
    
    // Remove member from group
    public function removeMember($userId) {
        return $this->db->table('group_members')
            ->where('group_id', $this->id)
            ->where('user_id', $userId)
            ->delete();
    }
    
    // Get group members
    public function getMembers() {
        $members = $this->db->table('group_members')
            ->select('users.*, group_members.status')
            ->join('users', 'users.id', '=', 'group_members.user_id')
            ->where('group_members.group_id', $this->id)
            ->where('group_members.status', 'active')
            ->get();
            
        $memberModels = [];
        foreach ($members as $member) {
            $memberModels[] = new User($member);
        }
        
        return $memberModels;
    }
    
    // Get group invitations
    public function getInvitations() {
        return $this->db->table('group_members')
            ->where('group_id', $this->id)
            ->where('status', 'invited')
            ->get();
    }
    
    // Find invitation by token
    public function findInvitationByToken($token) {
        return $this->db->table('group_members')
            ->where('group_id', $this->id)
            ->where('invitation_token', $token)
            ->where('status', 'invited')
            ->first();
    }
    
    // Accept invitation
    public function acceptInvitation($token, $userId) {
        $invitation = $this->findInvitationByToken($token);
        
        if (!$invitation) {
            return false;
        }
        
        return $this->db->table('group_members')
            ->where('id', $invitation['id'])
            ->update([
                'user_id' => $userId,
                'invitation_token' => null,
                'status' => 'active'
            ]);
    }
    
    // Add restriction (who can't gift whom)
    public function addRestriction($userId, $restrictedUserId) {
        // Check if restriction already exists
        $existingRestriction = $this->db->table('restrictions')
            ->where('group_id', $this->id)
            ->where('user_id', $userId)
            ->where('restricted_user_id', $restrictedUserId)
            ->first();
            
        if ($existingRestriction) {
            return false;
        }
        
        return $this->db->table('restrictions')->insert([
            'group_id' => $this->id,
            'user_id' => $userId,
            'restricted_user_id' => $restrictedUserId
        ]);
    }
    
    // Remove restriction
    public function removeRestriction($userId, $restrictedUserId) {
        return $this->db->table('restrictions')
            ->where('group_id', $this->id)
            ->where('user_id', $userId)
            ->where('restricted_user_id', $restrictedUserId)
            ->delete();
    }
    
    // Get restrictions for a user
    public function getRestrictions($userId) {
        $restrictions = $this->db->table('restrictions')
            ->select('restricted_user_id')
            ->where('group_id', $this->id)
            ->where('user_id', $userId)
            ->get();
            
        return array_column($restrictions, 'restricted_user_id');
    }
    
    // Get all restrictions
    public function getAllRestrictions() {
        $restrictions = $this->db->table('restrictions')
            ->where('group_id', $this->id)
            ->get();
            
        $result = [];
        foreach ($restrictions as $restriction) {
            $userId = $restriction['user_id'];
            $restrictedUserId = $restriction['restricted_user_id'];
            
            if (!isset($result[$userId])) {
                $result[$userId] = [];
            }
            
            $result[$userId][] = $restrictedUserId;
        }
        
        return $result;
    }
    
    // Perform the draw
    public function performDraw() {
        // Check if already drawn
        if ($this->isDrawn) {
            return false;
        }
        
        // Get active members
        $members = $this->db->table('group_members')
            ->select('user_id')
            ->where('group_id', $this->id)
            ->where('status', 'active')
            ->get();
            
        if (count($members) < 2) {
            return false; // Not enough members
        }
        
        // Extract user IDs
        $userIds = array_column($members, 'user_id');
        
        // Get restrictions
        $allRestrictions = $this->getAllRestrictions();
        
        // Start transaction
        $this->db->beginTransaction();
        
        try {
            // Clear any existing assignments
            $this->db->table('assignments')
                ->where('group_id', $this->id)
                ->delete();
                
            // Perform draw
            $assignments = $this->generateAssignments($userIds, $allRestrictions);
            
            // Insert assignments
            foreach ($assignments as $giverId => $receiverId) {
                $this->db->table('assignments')->insert([
                    'group_id' => $this->id,
                    'giver_id' => $giverId,
                    'receiver_id' => $receiverId
                ]);
            }
            
            // Mark group as drawn
            $this->db->table('groups')
                ->where('id', $this->id)
                ->update(['is_drawn' => true]);
                
            $this->db->commit();
            $this->isDrawn = true;
            
            return true;
        } catch (\Exception $e) {
            $this->db->rollBack();
            return false;
        }
    }
    
    // Generate assignments using algorithm that respects restrictions
    private function generateAssignments($userIds, $restrictions) {
        $givers = $userIds;
        $receivers = $userIds;
        $assignments = [];
        
        // Shuffle the receivers
        shuffle($receivers);
        
        // Keep track of assigned receivers
        $assignedReceivers = [];
        
        // First pass - handle the most restricted cases
        foreach ($givers as $giverId) {
            // Get restrictions for this giver
            $userRestrictions = $restrictions[$giverId] ?? [];
            
            // Add self to restrictions (can't gift self)
            $userRestrictions[] = $giverId;
            
            // Find available receivers
            $availableReceivers = array_filter($receivers, function($receiverId) use ($userRestrictions, $assignedReceivers) {
                return !in_array($receiverId, $userRestrictions) && !in_array($receiverId, $assignedReceivers);
            });
            
            // If no available receivers, we'll need to backtrack and reassign
            if (empty($availableReceivers)) {
                // Reset and try again with different shuffle
                return $this->generateAssignments($userIds, $restrictions);
            }
            
            // Pick a random receiver
            $randomIndex = array_rand($availableReceivers);
            $receiverId = $availableReceivers[$randomIndex];
            
            // Make assignment
            $assignments[$giverId] = $receiverId;
            $assignedReceivers[] = $receiverId;
        }
        
        return $assignments;
    }
    
    // Redraw assignments
    public function redraw() {
        // Reset drawn status
        $this->db->table('groups')
            ->where('id', $this->id)
            ->update(['is_drawn' => false]);
            
        $this->isDrawn = false;
        
        // Perform new draw
        return $this->performDraw();
    }
    
    // Get user's assignment
    public function getUserAssignment($userId) {
        $assignment = $this->db->table('assignments')
            ->where('group_id', $this->id)
            ->where('giver_id', $userId)
            ->first();
            
        if (!$assignment) {
            return null;
        }
        
        $userModel = new User();
        $receiver = $userModel->findById($assignment['receiver_id']);
        
        return $receiver;
    }
    
    // Get admin
    public function getAdmin() {
        $userModel = new User();
        return $userModel->findById($this->adminId);
    }
    
    // Check if user is admin
    public function isAdmin($userId) {
        return $this->adminId == $userId;
    }
    
    // Check if user is member
    public function isMember($userId) {
        $member = $this->db->table('group_members')
            ->where('group_id', $this->id)
            ->where('user_id', $userId)
            ->where('status', 'active')
            ->first();
            
        return $member !== false;
    }
    
    // Getters
    public function getId() {
        return $this->id;
    }
    
    public function getName() {
        return $this->name;
    }
    
    public function getDescription() {
        return $this->description;
    }
    
    public function getAdminId() {
        return $this->adminId;
    }
    
    public function getJoinDeadline() {
        return $this->joinDeadline;
    }
    
    public function getDrawDate() {
        return $this->drawDate;
    }
    
    public function getIsDrawn() {
        return $this->isDrawn;
    }
    
    public function getCustomEmailTemplate() {
        return $this->customEmailTemplate;
    }
    
    public function getWishlistVisibility() {
        return $this->wishlistVisibility;
    }
    
    public function getCreatedAt() {
        return $this->createdAt;
    }
    
    public function getUpdatedAt() {
        return $this->updatedAt;
    }
}
