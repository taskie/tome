import WorkspaceLink from "./WorkspaceLink";
import GroupLink from "./GroupLink";

type Props = {
  workspaceName: string;
  groupName?: string;
};

export const GlobalNav: React.FC<Props> = ({ workspaceName, groupName }) => {
  return (
    <nav>
      Workspace: <WorkspaceLink workspaceName={workspaceName} />
      {groupName != null ? (
        <>
          {" / "}
          Group: <GroupLink workspaceName={workspaceName} groupName={groupName} />
        </>
      ) : undefined}
    </nav>
  );
};

export default GlobalNav;
