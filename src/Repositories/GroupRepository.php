<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\Group;
use SecretSanta\Models\User;
use SecretSanta\Models\GiftAssignment;

/**
 * Repository class for handling Group data operations
 * 
 * Manages database interactions for group entities including CRUD operations,
 * member management, and gift assignment drawing functionality.
 */
class GroupRepository extends DataMapper
{
    /**
     * Database table name
     * 
     * @var string
     */
    protected string $table = 'groups';
    
    /**
     * Entity class name
     * 
     * @var string
     */
    protected string $entityClass = Group::class;
    
    /**
     * Available database columns
     * 
     * @var array
     */
    protected array $columns = [
        'id',
        'name',
        'description',
        'admin_id',
        'invitation_code',
        'registration_deadline',
        'draw_date',
        'is_drawn',
        'created_at',
        'updated_at'
    ];

    /**
     * Find a group by invitation code
     * 
     * @param string $code Invitation code to search for
     * @return Group|null Group entity if found, null otherwise
     */
    public function findByInvitationCode(string $code): ?Group
    {
        $groups = $this->findBy(['invitation_code' => $code]);
        return !empty($groups) ? $groups[0] : null;
    }

    /**
     * Find all groups administered by a specific user
     * 
     * @param int $adminId Admin user ID to search for
     * @return array Array of Group entities
     */
    public function findByAdminId(int $adminId): array
    {
        return $this->findBy(['admin_id' => $adminId]);
    }

    /**
     * Find all groups that a user belongs to
     * 
     * This method uses a JOIN with the group_members table to find groups
     * where the user is a member, regardless of admin status.
     * 
     * @param int $userId User ID to find groups for
     * @return array Array of Group entities
     */
    public function findByUserId(int $userId): array
    {
        // This requires a JOIN with the group_members table
        $sql = "
            SELECT g.* 
            FROM {$this->table} g
            JOIN group_members gm ON g.id = gm.group_id
            WHERE gm.user_id = ? 
            ORDER BY g.created_at DESC
        ";

        $stmt = $this->db->prepare($sql);
        $stmt->bind_param('i', $userId);
        $stmt->execute();
        $result = $stmt->get_result();
        
        $groups = [];
        while ($row = $result->fetch_assoc()) {
            $groups[] = $this->mapToEntity($row);
        }
        $stmt->close();
        
        return $groups;
    }

    /**
     * Load the admin user for a group
     * 
     * @param Group $group The group to load admin for
     * @return Group Group with admin user loaded
     */
    public function loadAdmin(Group $group): Group
    {
        if ($group->getAdminId() === null) {
            return $group;
        }

        $userRepository = new UserRepository();
        $admin = $userRepository->find($group->getAdminId());

        return $group->setAdmin($admin);
    }

    /**
     * Load all members of a group
     * 
     * @param Group $group The group to load members for
     * @return Group Group with members loaded
     */
    public function loadMembers(Group $group): Group
    {
        if ($group->getId() === null) {
            return $group;
        }

        $groupMemberRepository = new GroupMemberRepository();
        $userRepository = new UserRepository();

        $memberships = $groupMemberRepository->findBy(['group_id' => $group->getId()]);
        $members = [];

        foreach ($memberships as $membership) {
            $user = $userRepository->find($membership->getUserId());
            if ($user) {
                $members[] = $user;
            }
        }

        return $group->setMembers($members);
    }

    /**
     * Load all gift assignments for a group
     * 
     * @param Group $group The group to load assignments for
     * @return Group Group with assignments loaded
     */
    public function loadAssignments(Group $group): Group
    {
        if ($group->getId() === null) {
            return $group;
        }

        $assignmentRepository = new GiftAssignmentRepository();
        $assignments = $assignmentRepository->findBy(['group_id' => $group->getId()]);

        return $group->setAssignments($assignments);
    }

    /**
     * Load all exclusion rules for a group
     * 
     * @param Group $group The group to load exclusion rules for
     * @return Group Group with exclusion rules loaded
     */
    public function loadExclusionRules(Group $group): Group
    {
        if ($group->getId() === null) {
            return $group;
        }

        $exclusionRepository = new ExclusionRuleRepository();
        $exclusionRules = $exclusionRepository->findBy(['group_id' => $group->getId()]);

        return $group->setExclusionRules($exclusionRules);
    }

    /**
     * Create a new group
     * 
     * Automatically generates an invitation code and adds the creator as admin
     * and as the first group member.
     * 
     * @param User $admin User entity who will be the admin
     * @param string $name Group name
     * @param string|null $description Optional group description
     * @return Group The created group entity
     */
    public function createGroup(User $admin, string $name, ?string $description = null): Group
    {
        $group = new Group();
        $group->setName($name)
            ->setDescription($description)
            ->setAdminId($admin->getId())
            ->setInvitationCode($this->generateInvitationCode());

        $group = $this->save($group);

        // Add the admin as a member of the group
        $groupMemberRepository = new GroupMemberRepository();
        $groupMemberRepository->addMember($group->getId(), $admin->getId());

        return $group;
    }

    /**
     * Perform the Secret Santa gift assignment draw for a group
     * 
     * Creates gift assignments taking into account exclusion rules.
     * Uses multiple attempts to find a valid assignment configuration.
     * 
     * @param Group $group The group to perform draw for
     * @return bool True if draw was successful, false otherwise
     */
    public function performDraw(Group $group): bool
    {
        if ($group->isDrawn()) {
            return false;
        }

        // Load all members and exclusion rules
        $this->loadMembers($group);
        $this->loadExclusionRules($group);

        $members = $group->getMembers();
        $exclusionRules = $group->getExclusionRules();

        if (count($members) < 2) {
            return false;
        }

        // Build exclusion map
        $exclusionMap = [];
        foreach ($exclusionRules as $rule) {
            if (!isset($exclusionMap[$rule->getUserId()])) {
                $exclusionMap[$rule->getUserId()] = [];
            }
            $exclusionMap[$rule->getUserId()][] = $rule->getExcludedUserId();
        }

        // Perform the draw
        $assignments = $this->drawSecretSanta($members, $exclusionMap);

        if (empty($assignments)) {
            return false;
        }

        // Save assignments
        $assignmentRepository = new GiftAssignmentRepository();
        $this->beginTransaction();

        try {
            foreach ($assignments as $giverId => $receiverId) {
                $assignment = new GiftAssignment();
                $assignment->setGroupId($group->getId())
                    ->setGiverId($giverId)
                    ->setReceiverId($receiverId);
                $assignmentRepository->save($assignment);
            }

            // Mark group as drawn
            $group->setIsDrawn(true);
            $this->save($group);

            $this->commit();
            return true;
        } catch (\Exception $e) {
            $this->rollback();
            return false;
        }
    }

    /**
     * Algorithm to create Secret Santa assignments
     * 
     * Attempts to create a valid assignment where each person gives and receives
     * exactly one gift while respecting exclusion rules.
     * 
     * @param array $members Array of User entities
     * @param array $exclusionMap Map of user IDs to arrays of excluded user IDs
     * @return array Associative array of giver IDs to receiver IDs
     */
    private function drawSecretSanta(array $members, array $exclusionMap): array
    {
        $givers = array_map(function ($user) {
            return $user->getId();
        }, $members);
        $receivers = $givers;
        $assignments = [];

        // Shuffle the receivers for randomness
        shuffle($receivers);

        // Try to make valid assignments
        $maxAttempts = 10;
        for ($attempt = 0; $attempt < $maxAttempts; $attempt++) {
            $assignments = [];
            $tempReceivers = $receivers;
            $success = true;

            foreach ($givers as $giverId) {
                $validReceivers = array_filter($tempReceivers, function ($receiverId) use ($giverId, $exclusionMap, $assignments) {
                    // Can't be assigned to self
                    if ($receiverId === $giverId) {
                        return false;
                    }

                    // Can't be assigned to someone in exclusion list
                    if (isset($exclusionMap[$giverId]) && in_array($receiverId, $exclusionMap[$giverId])) {
                        return false;
                    }

                    // Check for reciprocal assignments (A->B and B->A)
                    if (isset($assignments[$receiverId]) && $assignments[$receiverId] === $giverId) {
                        return false;
                    }

                    return true;
                });

                if (empty($validReceivers)) {
                    $success = false;
                    break;
                }

                // Get a random valid receiver
                $randomIndex = array_rand($validReceivers);
                $receiverId = $validReceivers[$randomIndex];

                $assignments[$giverId] = $receiverId;

                // Remove assigned receiver from available receivers
                $key = array_search($receiverId, $tempReceivers);
                if ($key !== false) {
                    unset($tempReceivers[$key]);
                }
            }

            if ($success) {
                return $assignments;
            }
        }

        return [];
    }

    /**
     * Generate a unique invitation code for a group
     * 
     * Creates an 8-character alphanumeric code (excluding similar-looking characters)
     * and ensures it is unique among existing groups.
     * 
     * @return string Unique invitation code
     */
    private function generateInvitationCode(): string
    {
        do {
            $code = substr(str_shuffle('ABCDEFGHJKLMNPQRSTUVWXYZ23456789'), 0, 8);
            $existing = $this->findByInvitationCode($code);
        } while ($existing !== null);

        return $code;
    }
}
